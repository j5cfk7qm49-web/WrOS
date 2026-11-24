#!/usr/bin/env python3
"""Measure PDF rendering and OCR on Linux with disposable files."""

import argparse
import ctypes as ct
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import signal
import statistics
import subprocess
import sys
import tempfile
import time
import unicodedata
import zlib


def pdfium_render(library, path, out, page_number, dpi, gray):
    api = ct.CDLL(str(library))
    ptr, integer, single = ct.c_void_p, ct.c_int, ct.c_float

    def symbol(name, result, *arguments):
        function = getattr(api, name)
        function.restype = result
        function.argtypes = arguments
        return function

    init = symbol("FPDF_InitLibrary", None)
    destroy = symbol("FPDF_DestroyLibrary", None)
    load = symbol("FPDF_LoadDocument", ptr, ct.c_char_p, ct.c_char_p)
    load_page = symbol("FPDF_LoadPage", ptr, ptr, integer)
    close_document = symbol("FPDF_CloseDocument", None, ptr)
    close_page = symbol("FPDF_ClosePage", None, ptr)
    width_of = symbol("FPDF_GetPageWidthF", single, ptr)
    height_of = symbol("FPDF_GetPageHeightF", single, ptr)
    create = symbol("FPDFBitmap_CreateEx", ptr, integer, integer, integer, ptr, integer)
    release_bitmap = symbol("FPDFBitmap_Destroy", None, ptr)
    pixels_of = symbol("FPDFBitmap_GetBuffer", ptr, ptr)
    stride_of = symbol("FPDFBitmap_GetStride", integer, ptr)
    fill = symbol("FPDFBitmap_FillRect", integer, ptr, integer, integer, integer, integer, ct.c_ulong)
    render = symbol("FPDF_RenderPageBitmap", None, ptr, ptr, integer, integer,
                    integer, integer, integer, integer)
    document = page = bitmap = None
    init()
    try:
        document = load(os.fsencode(path), None)
        if not document:
            raise ValueError("PDFium could not open the PDF")
        page = load_page(document, page_number - 1)
        if not page:
            raise ValueError(f"PDFium could not open page {page_number}")
        width = math.ceil(width_of(page) * dpi / 72)
        height = math.ceil(height_of(page) * dpi / 72)
        if width <= 0 or height <= 0 or width * height > 36_000_000:
            raise ValueError("Rendered page exceeds 36 megapixels")
        bitmap = create(width, height, 1 if gray else 3, None, 0)
        if not bitmap:
            raise ValueError("PDFium could not allocate the image")
        fill(bitmap, 0, 0, width, height, 0xFFFFFFFF)
        render(bitmap, page, 0, 0, width, height, 0, 0x08 if gray else 0)
        pixels, stride = pixels_of(bitmap), stride_of(bitmap)
        with out.open("wb") as file:
            file.write(f"{'P5' if gray else 'P6'}\n{width} {height}\n255\n".encode())
            for line in range(height):
                offset = pixels + line * stride
                if gray:
                    file.write(ct.string_at(offset, width))
                else:
                    bgrx = ct.string_at(offset, width * 4)
                    rgb = bytearray(width * 3)
                    rgb[0::3] = bgrx[2::4]
                    rgb[1::3] = bgrx[1::4]
                    rgb[2::3] = bgrx[0::4]
                    file.write(rgb)
    finally:
        if bitmap:
            release_bitmap(bitmap)
        if page:
            close_page(page)
        if document:
            close_document(document)
        destroy()


def timed(args, deadline, stdout=None, memory_mib=1024):
    started = time.monotonic()
    def limit_memory():
        memory = memory_mib * 1024 * 1024
        resource.setrlimit(resource.RLIMIT_AS, (memory, memory))

    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(
            [os.fspath(arg) for arg in args],
            stdin=subprocess.DEVNULL, stdout=stdout or subprocess.DEVNULL,
            stderr=errors, start_new_session=True,
            preexec_fn=limit_memory,
        )
        while True:
            pid, status, usage = os.wait4(process.pid, os.WNOHANG)
            if pid:
                process.returncode = os.waitstatus_to_exitcode(status)
                break
            if time.monotonic() - started >= deadline:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                _, status, _ = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
                raise TimeoutError(f"{Path(args[0]).name} exceeded {deadline} seconds")
            time.sleep(0.005)
        elapsed = time.monotonic() - started
        if process.returncode:
            errors.seek(0)
            raise RuntimeError(errors.read(2048).decode(errors="replace").strip()
                               or f"{Path(args[0]).name} exited with {process.returncode}")
    return {"seconds": round(elapsed, 4), "peak_rss_kib": usage.ru_maxrss}


def render_command(args, engine, pdf, raster, page, dpi, gray):
    if engine == "PDFium":
        return [sys.executable, Path(__file__), "pdfium-render", args.pdfium, pdf, raster,
                str(page), str(dpi), "gray" if gray else "rgb"]
    if engine == "Poppler":
        command = [args.pdftoppm, "-f", str(page), "-l", str(page), "-r", str(dpi), "-singlefile"]
        if gray:
            command.append("-gray")
        return command + [pdf, raster.with_suffix("")]
    return [args.hayro, pdf, raster, str(page), str(dpi), "gray" if gray else "rgb"]


def normalized(text, language):
    words = unicodedata.normalize("NFKC", text).split()
    return ("" if language == "Chinese" else " ").join(words)


def character_error_rate(reference, recognized, language):
    reference = normalized(reference, language)
    recognized = normalized(recognized, language)
    if not reference or max(len(reference), len(recognized)) > 5000:
        raise ValueError("Accuracy transcripts must contain 1–5,000 characters")
    previous = list(range(len(recognized) + 1))
    for row, expected in enumerate(reference, 1):
        current = [row]
        for col, actual in enumerate(recognized, 1):
            current.append(min(current[-1] + 1, previous[col] + 1,
                               previous[col - 1] + (expected != actual)))
        previous = current
    return round(previous[-1] / len(reference), 5)


def file_hash(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def machine():
    info = Path("/proc/cpuinfo").read_text()
    first = {
        key.strip(): value.strip()
        for line in info.split("\n\n", 1)[0].splitlines()
        if ":" in line
        for key, value in [line.split(":", 1)]
    }
    return {
        "cpu": first.get("model name", platform.processor()),
        "cpu_flags": first.get("flags", "").split(),
        "kernel": platform.release(),
        "memory_kib": int(Path("/proc/meminfo").read_text().splitlines()[0].split()[1]),
    }


def run(args):
    if sys.platform != "linux":
        raise ValueError("Peak RSS and child accounting require Linux")
    engines = ["PDFium", "Poppler"] + (["Hayro"] if args.hayro else [])
    corpus = [
        ("English", args.english, args.english_page, "eng", args.english_truth),
        ("Chinese", args.chinese, args.chinese_page, args.chinese_language, args.chinese_truth),
    ]
    report = {
        "schema": 1,
        "machine": machine(),
        "renderer": {"PDFium_sha256": file_hash(args.pdfium), "Poppler": Path(args.pdftoppm).name,
                     "Hayro": Path(args.hayro).name if args.hayro else None},
        "ocr": {"program": Path(args.tesseract).name, "custom_models": args.tessdata is not None},
        "limits": {"seconds_per_process": args.timeout, "address_space_mib": args.memory_mib},
        "samples": [],
    }
    with tempfile.TemporaryDirectory(prefix="wros-pdf-ocr-") as directory:
        work = Path(directory)
        for language, pdf, page, model, truth in corpus:
            render_trials, ocr_trials = [], []
            sample = {"language": language, "sha256": file_hash(pdf), "page": page,
                      "render": render_trials, "ocr": ocr_trials}
            for engine in engines:
                for gray in (False, True):
                    raster = work / ("page.pgm" if gray else "page.ppm")
                    command = render_command(args, engine, pdf, raster, page, args.render_dpi, gray)
                    trials = []
                    for _ in range(args.trials):
                        raster.unlink(missing_ok=True)
                        trial = timed(command, args.timeout, memory_mib=args.memory_mib)
                        if raster.stat().st_size < 32:
                            raise ValueError(f"{engine} returned an empty image")
                        trial["raster_bytes"] = raster.stat().st_size
                        trials.append(trial)
                    render_trials.append({
                        "engine": engine, "dpi": args.render_dpi, "format": "gray" if gray else "rgb",
                        "trials": trials, "median_seconds": statistics.median(t["seconds"] for t in trials),
                        "peak_rss_kib": max(t["peak_rss_kib"] for t in trials),
                    })
            for dpi in (200, 300):
                raster = work / "ocr.pgm"
                raster.unlink(missing_ok=True)
                timed(render_command(args, "PDFium", pdf, raster, page, dpi, True),
                      args.timeout, memory_mib=args.memory_mib)
                cmd = [args.tesseract, raster, "stdout", "-l", model, "--dpi", str(dpi), "--psm", "6"]
                if args.tessdata:
                    cmd += ["--tessdata-dir", args.tessdata]
                out = work / "ocr.txt"
                trials = []
                for _ in range(args.trials):
                    with out.open("wb") as stream:
                        trial = timed(cmd, args.timeout, stream, args.memory_mib)
                    if truth:
                        trial["character_error_rate"] = character_error_rate(
                            truth.read_text(), out.read_text(errors="replace"), language,
                        )
                    trials.append(trial)
                ocr_trials.append({
                    "model": model, "dpi": dpi, "trials": trials,
                    "median_seconds": statistics.median(t["seconds"] for t in trials),
                    "peak_rss_kib": max(t["peak_rss_kib"] for t in trials),
                })
            report["samples"].append(sample)
    output = args.output
    with tempfile.NamedTemporaryFile(mode="w", dir=output.parent, delete=False) as stream:
        temporary = Path(stream.name)
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    temporary.replace(output)


def write_image_pdf(path, width, height, pixels):
    compressed = zlib.compress(pixels)
    page_width = round(width * 72 / 200)
    page_height = round(height * 72 / 200)
    content = f"q {page_width} 0 0 {page_height} 0 0 cm /Image Do Q\n".encode()
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        (f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width} {page_height}] "
         "/Resources << /XObject << /Image 4 0 R >> >> /Contents 5 0 R >>").encode(),
        (f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} "
         f"/ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {len(compressed)} >>\n"
         "stream\n").encode() + compressed + b"\nendstream",
        f"<< /Length {len(content)} >>\nstream\n".encode() + content + b"endstream",
    ]
    data = bytearray(b"%PDF-1.4\n%\xff\xff\xff\xff\n")
    offsets = [0]
    for index, body in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f"{index} 0 obj\n".encode() + body + b"\nendobj\n")
    xref = len(data)
    data.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        data.extend(f"{offset:010d} 00000 n \n".encode())
    data.extend((f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\n"
                 f"startxref\n{xref}\n%%EOF\n").encode())
    path.write_bytes(data)


def fixtures(font, output):
    paragraphs = {
        "english": [
            "The library opened beside the garden.",
            "Several readers brought notes and books.",
            "They studied the maps of the old roads.",
            "A quiet desk gave them room to write.",
        ],
        "chinese": [
            "图书馆坐落在花园旁边。",
            "读者带着笔记和书来到这里。",
            "他们一起研究古老道路的地图。",
            "安静的书桌让大家专心写作。",
        ],
    }
    needed = {ord(c) for lines in paragraphs.values() for text in lines for c in text}
    glyphs = {}
    with gzip.open(font, "rt", encoding="ascii") as stream:
        for line in stream:
            code, bitmap = line.strip().split(":", 1)
            if (number := int(code, 16)) in needed:
                glyphs[number] = bytes.fromhex(bitmap)
    if glyphs.keys() != needed:
        raise ValueError("The font lacks a fixture character")
    output.mkdir(parents=True, exist_ok=True)
    for tag, lines in paragraphs.items():
        width, height = 1050, 310
        pixels = bytearray(b"\xff" * (width * height))
        for line, text in enumerate(lines):
            x, y = 60, 42 + line * 54
            for char in text:
                glyph = glyphs[ord(char)]
                stride = len(glyph) // 16
                for row in range(16):
                    for col in range(stride * 8):
                        part = glyph[row * stride + col // 8]
                        if part & (0x80 >> (col % 8)):
                            for dy in range(3):
                                offset = (y + 3 * row + dy) * width + x + 3 * col
                                pixels[offset:offset + 3] = b"\0\0\0"
                x += stride * 24
        write_image_pdf(output / f"{tag}.pdf", width, height, pixels)
        (output / f"{tag}.txt").write_text("\n".join(lines) + "\n")


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    benchmark = sub.add_parser("run", help="measure the selected PDF pages")
    benchmark.add_argument("--pdfium", type=Path, required=True, help="path to libpdfium.so")
    benchmark.add_argument("--pdftoppm", default="pdftoppm")
    benchmark.add_argument("--tesseract", default="tesseract")
    benchmark.add_argument("--hayro", help="optional PDF, output, page, DPI, gray/rgb renderer")
    benchmark.add_argument("--tessdata", type=Path)
    for language in ("english", "chinese"):
        benchmark.add_argument(f"--{language}", type=Path, required=True)
        benchmark.add_argument(f"--{language}-page", type=int, default=1)
        benchmark.add_argument(f"--{language}-truth", type=Path)
    benchmark.add_argument("--chinese-language", choices=("chi_sim", "chi_tra", "chi_sim_vert", "chi_tra_vert"),
                           default="chi_sim")
    benchmark.add_argument("--render-dpi", type=int, default=96)
    benchmark.add_argument("--trials", type=int, default=3)
    benchmark.add_argument("--timeout", type=int, default=120)
    benchmark.add_argument("--memory-mib", type=int, default=1024)
    benchmark.add_argument("--output", type=Path, required=True)
    render = sub.add_parser("pdfium-render", help=argparse.SUPPRESS)
    render.add_argument("library", type=Path)
    render.add_argument("pdf", type=Path)
    render.add_argument("raster", type=Path)
    render.add_argument("page", type=int)
    render.add_argument("dpi", type=int)
    render.add_argument("format", choices=("gray", "rgb"))
    fixture = sub.add_parser("fixtures", help="make small scanned-page fixtures from Unifont")
    fixture.add_argument("--unifont", type=Path, required=True)
    fixture.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main():
    args = parse_args()
    if args.command == "pdfium-render":
        pdfium_render(args.library, args.pdf, args.raster, args.page, args.dpi, args.format == "gray")
    elif args.command == "fixtures":
        fixtures(args.unifont, args.output)
    else:
        if not (1 <= args.trials <= 20 and 30 <= args.render_dpi <= 600 and args.timeout > 0
                and 128 <= args.memory_mib <= 4096 and args.english_page > 0
                and args.chinese_page > 0):
            raise ValueError("Check the trial, page, DPI and timeout values")
        run(args)


if __name__ == "__main__":
    main()
