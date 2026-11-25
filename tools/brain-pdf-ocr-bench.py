#!/usr/bin/env python3
"""Measure PDF rendering and OCR on Linux with disposable files."""

import argparse
from contextlib import contextmanager
import ctypes as ct
from datetime import date
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


def pdfium_symbols(api):
    def symbol(name, result, *arguments):
        function = getattr(api, name)
        function.restype = result
        function.argtypes = arguments
        return function

    return symbol


@contextmanager
def pdfium_page(library, path, page_number):
    started = time.monotonic()
    symbol = pdfium_symbols(ct.CDLL(str(library)))
    library_load_ms = (time.monotonic() - started) * 1000
    ptr, integer = ct.c_void_p, ct.c_int
    init = symbol("FPDF_InitLibrary", None)
    destroy = symbol("FPDF_DestroyLibrary", None)
    load = symbol("FPDF_LoadDocument", ptr, ct.c_char_p, ct.c_char_p)
    load_page = symbol("FPDF_LoadPage", ptr, ptr, integer)
    close_document = symbol("FPDF_CloseDocument", None, ptr)
    close_page = symbol("FPDF_ClosePage", None, ptr)
    document = page = None
    started = time.monotonic()
    init()
    try:
        document = load(os.fsencode(path), None)
        if not document:
            raise ValueError("PDFium could not open the PDF")
        page = load_page(document, page_number - 1)
        if not page:
            raise ValueError(f"PDFium could not open page {page_number}")
        yield symbol, page, {
            "library_load_ms": round(library_load_ms, 3),
            "init_and_open_ms": round((time.monotonic() - started) * 1000, 3),
        }
    finally:
        if page:
            close_page(page)
        if document:
            close_document(document)
        destroy()


def pdfium_dimensions(symbol, page):
    width_of = symbol("FPDF_GetPageWidthF", ct.c_float, ct.c_void_p)
    height_of = symbol("FPDF_GetPageHeightF", ct.c_float, ct.c_void_p)
    return width_of(page), height_of(page)


@contextmanager
def pdfium_bitmap(symbol, page, width, height, pixel_format):
    ptr, integer = ct.c_void_p, ct.c_int
    create = symbol("FPDFBitmap_CreateEx", ptr, integer, integer, integer, ptr, integer)
    release_bitmap = symbol("FPDFBitmap_Destroy", None, ptr)
    pixels_of = symbol("FPDFBitmap_GetBuffer", ptr, ptr)
    stride_of = symbol("FPDFBitmap_GetStride", integer, ptr)
    fill = symbol("FPDFBitmap_FillRect", integer, ptr, integer, integer, integer, integer, ct.c_ulong)
    render = symbol("FPDF_RenderPageBitmap", None, ptr, ptr, integer, integer,
                    integer, integer, integer, integer)
    if min(width, height) <= 0 or width * height > 36_000_000:
        raise ValueError("Rendered page exceeds 36 megapixels")
    bitmap = create(width, height, pixel_format, None, 0)
    if not bitmap:
        raise ValueError("PDFium could not allocate the image")
    try:
        fill(bitmap, 0, 0, width, height, 0xFFFFFFFF)
        render(bitmap, page, 0, 0, width, height, 0, 0x08 if pixel_format == 1 else 0)
        yield pixels_of(bitmap), stride_of(bitmap)
    finally:
        release_bitmap(bitmap)


def pdfium_render(library, path, out, page_number, dpi, gray):
    with pdfium_page(library, path, page_number) as (symbol, page, _):
        width, height = (math.ceil(size * dpi / 72) for size in pdfium_dimensions(symbol, page))
        with pdfium_bitmap(symbol, page, width, height, 1 if gray else 3) as (pixels, stride):
            write_pnm(out, width, height, pixels, stride, gray)


def write_pnm(out, width, height, pixels, stride, gray):
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


def timed(args, deadline, stdout=None, memory_mib=1024, *, sample=None, low_priority=False):
    started = time.monotonic()

    def limit_memory():
        memory = memory_mib * 1024 * 1024
        resource.setrlimit(resource.RLIMIT_AS, (memory, memory))
        if low_priority:
            os.nice(10)

    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(
            [os.fspath(arg) for arg in args],
            stdin=subprocess.DEVNULL, stdout=stdout or subprocess.DEVNULL,
            stderr=errors, start_new_session=True,
            preexec_fn=limit_memory,
        )
        try:
            while True:
                pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                if pid:
                    process.returncode = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() - started >= deadline:
                    raise TimeoutError(f"{Path(args[0]).name} exceeded {deadline:g} seconds")
                if sample:
                    sample()
                time.sleep(0.005)
        finally:
            if process.returncode is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                _, status, _ = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
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


HP_MEASUREMENTS = ("hardware", "smoke", "start", "render", "extract", "ocr", "sustained")
HP_MODELS = ("fast", "standard", "best")
FIT_WIDTH = 1366


def pdfium_text(symbol, page):
    ptr, integer = ct.c_void_p, ct.c_int
    load = symbol("FPDFText_LoadPage", ptr, ptr)
    close = symbol("FPDFText_ClosePage", None, ptr)
    count = symbol("FPDFText_CountChars", integer, ptr)
    read = symbol("FPDFText_GetText", integer, ptr, integer, integer, ct.POINTER(ct.c_ushort))
    text_page = load(page)
    if not text_page:
        raise ValueError("PDFium could not load the text page")
    try:
        size = min(count(text_page), 5000)
        if size <= 0:
            return ""
        buffer = (ct.c_ushort * (2 * size + 1))()
        read(text_page, 0, size, buffer)
        return bytes(buffer).decode("utf-16-le", errors="replace").split("\0", 1)[0]
    finally:
        close(text_page)


def pdfium_sample(args):
    with pdfium_page(args.library, args.pdf, args.page) as (symbol, page, phases):
        if args.format == "text":
            sys.stdout.buffer.write(pdfium_text(symbol, page).encode("utf-8"))
            return
        width, height = pdfium_dimensions(symbol, page)
        if width <= 0:
            raise ValueError("Invalid page width")
        height = math.ceil(height * FIT_WIDTH / width)
        timings = []
        for index in range(args.trials):
            started = time.monotonic()
            with pdfium_bitmap(symbol, page, FIT_WIDTH, height, 1 if args.format == "gray" else 4) as (pixels, stride):
                timings.append(round((time.monotonic() - started) * 1000, 3))
                if index == 0:
                    buffer = (ct.c_ubyte * (stride * height)).from_address(pixels)
                    phases["raster_sha256"] = hashlib.sha256(buffer).hexdigest()
        phases.update(width=FIT_WIDTH, height=height, dpi=max(1, round(FIT_WIDTH * 72 / width)),
                      first_page_ms=timings[0], warm_ms=timings[1:])
        print(json.dumps(phases))


def hp_manifest(corpus, verify=False):
    manifest = json.loads((corpus / "MANIFEST.json").read_text(encoding="utf-8"))
    if verify:
        hashes = manifest["sha256"]
        files = {path.name for path in corpus.iterdir() if path.is_file()} - {"MANIFEST.json"}
        if not hashes or hashes.keys() != files or any(file_hash(corpus / name) != digest
                                                      for name, digest in hashes.items()):
            raise ValueError("Corpus files do not match MANIFEST.json")
    return manifest


def cold_hint(*paths):
    try:
        for path in paths:
            with path.open("rb") as stream:
                os.posix_fadvise(stream.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
        return True
    except OSError:
        return False


def storage_device(path):
    device = path.stat().st_dev
    sysfs = Path(f"/sys/dev/block/{os.major(device)}:{os.minor(device)}")
    return sysfs.resolve().name if sysfs.exists() else None


def read_sysfs(path):
    try:
        return path.read_text().strip()
    except OSError:
        return None


def hp_hardware(corpus, output):
    return {
        **machine(),
        "corpus_device": storage_device(corpus),
        "report_device": storage_device(output.parent),
        "emmc": [{"device": path.name, "name": read_sysfs(path / "device/name"),
                  "sectors_512b": read_sysfs(path / "size")}
                 for path in sorted(Path("/sys/block").glob("mmcblk[0-9]"))],
    }


def hp_timed(args, command, deadline, out=None, **options):
    remaining = min(args.timeout, deadline - time.monotonic())
    if remaining <= 0:
        raise TimeoutError("Measurement window elapsed")
    if out is None:
        return timed(command, remaining, memory_mib=args.memory_mib, **options)
    with out.open("wb") as stream:
        return timed(command, remaining, stream, args.memory_mib, **options)


def hp_render(args, pdf, page, work, deadline, measurements):
    result = []
    details = work / "render.json"
    formats = ("gray", "bgra") if "render" in measurements else ("gray",)
    for fmt in formats:
        cold = cold_hint(pdf, args.pdfium)
        command = [sys.executable, Path(__file__), "pdfium-sample", args.pdfium, pdf,
                   str(page), fmt]
        trial = hp_timed(args, [*command, "1"], deadline, details)
        phases = json.loads(details.read_text())
        entry = {"engine": "PDFium", "format": fmt, "cold_hint_accepted": cold,
                 "cold": {**trial, **phases}}
        if "render" in measurements:
            trial = hp_timed(args, [*command, str(1 + args.trials)], deadline, details)
            warm = json.loads(details.read_text())
            entry["warm"] = {**trial, **warm, "median_ms": statistics.median(warm["warm_ms"])}
        result.append(entry)
    if "render" in measurements:
        for engine in ["Poppler"] + (["Hayro"] if args.hayro else []):
            for gray in (True, False):
                raster = work / ("render.pgm" if gray else "render.ppm")
                command = render_command(args, engine, pdf, raster, page, phases["dpi"], gray)
                if engine == "Poppler":
                    command[5:7] = ["-scale-to-x", str(FIT_WIDTH), "-scale-to-y", "-1"]
                trials = []
                for _ in range(args.trials):
                    raster.unlink(missing_ok=True)
                    trial = hp_timed(args, command, deadline)
                    trial["raster_bytes"] = raster.stat().st_size
                    trials.append(trial)
                result.append({"engine": engine, "format": "gray" if gray else "rgb",
                               "trials": trials, "median_ms": 1000 * statistics.median(t["seconds"] for t in trials),
                               "peak_rss_kib": max(t["peak_rss_kib"] for t in trials)})
    return result


def hp_extract(args, pdf, page, truth, language, work, deadline):
    number = str(page)
    out = work / "extract.txt"
    commands = {
        "PDFium": [sys.executable, Path(__file__), "pdfium-sample", args.pdfium,
                   pdf, number, "text", "1"],
        "Poppler": [args.pdftotext, "-f", number, "-l", number, "-enc", "UTF-8", pdf, "-"],
    }
    results = []
    for engine, command in commands.items():
        trial = hp_timed(args, command, deadline, out)
        text = out.read_text(encoding="utf-8", errors="replace")
        trial.update(engine=engine, **text_check(truth, text, language))
        results.append(trial)
    return results


def text_check(truth, text, language):
    result = {"characters": len(normalized(text, language))}
    if truth is not None:
        try:
            result["character_error_rate"] = character_error_rate(truth, text, language)
        except ValueError:
            result["character_error_rate"] = None
    return result


def hp_ocr_command(args, raster, model, dpi, psm, data):
    return [args.tesseract, raster, "stdout", "-l", model, "--dpi", str(dpi),
            "--oem", "1", "--psm", str(psm), "--tessdata-dir", data]


def hp_ocr(args, case, pdf, page, truth, work, deadline):
    results = []
    models = dict(args.model_dir)
    model = case.get("model", "eng")
    tiers = HP_MODELS if model == "eng" else ("fast",)
    raster = work / "ocr.pgm"
    out = work / "ocr.txt"
    for dpi in (200, 300):
        hp_timed(args, render_command(args, "PDFium", pdf, raster, page, dpi, True), deadline)
        with raster.open("r+b") as stream:
            os.fsync(stream.fileno())
        for tier in tiers:
            traineddata = models[tier] / f"{model}.traineddata"
            if not traineddata.is_file():
                raise ValueError(f"Missing {tier} {model} traineddata")
            command = hp_ocr_command(args, raster, model, dpi, case.get("psm", 3), models[tier])
            accepted = cold_hint(traineddata, raster)
            cold = hp_timed(args, command, deadline, out)
            warm = [hp_timed(args, command, deadline) for _ in range(args.trials)]
            text = out.read_text(encoding="utf-8", errors="replace")
            results.append({
                "tier": tier if model == "eng" else "packaged",
                "model": model, "model_sha256": file_hash(traineddata), "dpi": dpi,
                "cold_hint_accepted": accepted, "cold": cold, "warm": warm,
                "warm_median_s": statistics.median(t["seconds"] for t in warm),
                "peak_rss_kib": max(t["peak_rss_kib"] for t in [cold, *warm]),
                **text_check(truth, text, case.get("language", "English")),
            })
    return results


def ocr_load(args, command, deadline, monitor=False):
    started = time.monotonic()
    next_sample = started
    temperatures, times = [], []
    zones = {zone: read_sysfs(zone / "type") for zone in Path("/sys/class/thermal").glob("thermal_zone*")}
    clocks = sorted(Path("/sys/devices/system/cpu/cpufreq").glob("policy*/scaling_cur_freq"))

    def sample():
        nonlocal next_sample
        now = time.monotonic()
        if now >= next_sample:
            temperatures.append({
                "elapsed_s": round(now - started, 2),
                "completed_pages": len(times),
                "thermal_millidegrees_c": {f"{zone.name}:{kind}": read_sysfs(zone / "temp")
                                           for zone, kind in sorted(zones.items())},
                "cpu_khz": {path.parent.name: read_sysfs(path) for path in clocks},
            })
            next_sample = now + 10

    while time.monotonic() < deadline:
        try:
            times.append(hp_timed(args, command, deadline, sample=sample if monitor else None,
                                  low_priority=True))
        except TimeoutError:
            if time.monotonic() < deadline:
                raise
            break
    return {"elapsed_s": round(time.monotonic() - started, 2), "completed_pages": len(times),
            "median_seconds_per_page": statistics.median(t["seconds"] for t in times) if times else None,
            "peak_rss_kib": max((t["peak_rss_kib"] for t in times), default=None),
            "telemetry": temperatures}


def hp_sample(args, case, pdf, page, work, deadline, measurements, smoke):
    private = "private" in case
    result = {"page": page, "source": "owner" if private else case["pdf"]}
    truth = None
    if page == 1 and case.get("truth"):
        truth = (args.corpus / case["truth"]).read_text(encoding="utf-8")

    def measure(name, function, *values):
        try:
            result[name] = function(*values)
        except (OSError, ValueError, RuntimeError) as error:
            result[name] = {"error": type(error).__name__ if private else str(error)}

    if smoke or measurements & {"start", "render"}:
        measure("render", hp_render, args, pdf, page, work, deadline, measurements)
    if smoke or "extract" in measurements:
        measure("extract", hp_extract, args, pdf, page, None if case.get("codec") else truth,
                case.get("language", "English"), work, deadline)
    scan_in_full = not args.quick and case.get("codec")
    if "ocr" in measurements and (case.get("ocr") or scan_in_full or private):
        measure("ocr", hp_ocr, args, case, pdf, page, truth, work, deadline)
    if private:
        measure("sha256", file_hash, pdf)
    return result


def hp_load(args, work, deadline=None, duration=None):
    raster = work / "sustained.pgm"
    scan = args.corpus / "book-scan-gray.pdf"
    try:
        render_deadline = deadline if deadline is not None else time.monotonic() + args.timeout
        hp_timed(args, render_command(args, "PDFium", scan, raster, 1, 300, True), render_deadline)
        data = dict(args.model_dir)["fast"]
        command = hp_ocr_command(args, raster, "eng", 300, 3, data)
        if duration is not None:
            deadline = time.monotonic() + duration
        result = ocr_load(args, command, deadline, monitor=duration is not None)
        result.update(dpi=300, model="eng", tier="fast", page=1, source=scan.name,
                      model_sha256=file_hash(data / "eng.traineddata"))
        return result
    except (OSError, ValueError, RuntimeError) as error:
        return {"error": str(error)}


def hp_run(args):
    manifest = hp_manifest(args.corpus, args.verify_corpus)
    if args.verify_corpus:
        print(f"Verified {len(manifest['sha256'])} corpus hashes")
        return
    if sys.platform != "linux":
        raise ValueError("Peak RSS and child accounting require Linux")
    os.environ["OMP_THREAD_LIMIT"] = "1"
    started = time.monotonic()
    profile_seconds = 600 if args.quick else 1800
    deadline = started + profile_seconds
    measurements = set(args.measure or HP_MEASUREMENTS)
    if args.quick and not args.measure:
        measurements.remove("sustained")
    output = args.output or Path.home() / f"hp-measure-{date.today()}.json"
    samples = []
    report = {"schema": 1, "profile": "quick" if args.quick else "full", "samples": samples,
              "selected": sorted(measurements), "corpus": manifest,
              "tools": {"pdfium": str(args.pdfium), "pdftoppm": args.pdftoppm,
                        "pdftotext": args.pdftotext, "tesseract": args.tesseract, "hayro": args.hayro},
              "settings": {"fit_width_px": FIT_WIDTH, "trials": args.trials, "process_timeout_s": args.timeout,
                           "memory_mib_per_process": args.memory_mib, "omp_thread_limit": 1, "tesseract_oem": 1}}
    if "hardware" in measurements:
        report["machine"] = hp_hardware(args.corpus, output)
    cases = manifest["cases"].copy()
    if args.pdf_dir:
        cases += [{"private": path} for path in sorted(args.pdf_dir.iterdir())
                  if path.is_file() and path.suffix.lower() == ".pdf"]
    page_measures = measurements & {"smoke", "start", "render", "extract", "ocr"}
    with tempfile.TemporaryDirectory(prefix=".wros-hp-measure-", dir=output.parent) as directory:
        work = Path(directory)
        for case in cases:
            smoke = "smoke" in measurements and case.get("smoke")
            private = case.get("private")
            if not (smoke or page_measures - {"smoke"} or private):
                continue
            pdf = private or args.corpus / case["pdf"]
            for page in case.get("pages", [1])[:1 if args.quick else None]:
                sample = hp_sample(args, case, pdf, page, work, deadline, measurements, smoke)
                if not private:
                    sample["sha256"] = manifest["sha256"][case["pdf"]]
                samples.append(sample)
                if smoke:
                    report["smoke"] = (isinstance(sample["render"], list)
                                       and isinstance(sample["extract"], list)
                                       and sample["extract"][0]["characters"] > 0)
        if page_measures or "sustained" in measurements:
            report["tools"]["pdfium_sha256"] = file_hash(args.pdfium)
        if "sustained" in measurements:
            before = time.monotonic()
            report["sustained"] = hp_load(args, work, duration=60 if args.quick else 1800)
            deadline += time.monotonic() - before
        if not args.measure and deadline - time.monotonic() > 1:
            report["warm_ocr_window"] = hp_load(args, work, deadline=deadline)
    report["elapsed_s"] = round(time.monotonic() - started, 2)
    write_report(report, output)
    errors = sum(isinstance(value, dict) and "error" in value
                 for result in [report, *samples] for value in result.values())
    errors += report.get("smoke") is False
    print(f"HP Stream {report['profile']}: {len(samples)} pages, {errors} failed cases, "
          f"{report['elapsed_s'] / 60:.1f} min.  {output}")
    return int(errors > 0)


def write_report(report, output):
    stream = tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=output.parent, delete=False)
    temporary = Path(stream.name)
    try:
        with stream:
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)


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
    write_report(report, args.output)


def write_image_pdf(path, width, height, pixels, *, dpi=200, jbig2=False):
    compressed = pixels if jbig2 else zlib.compress(pixels)
    pdf_filter = "JBIG2Decode" if jbig2 else "FlateDecode"
    bits = 1 if jbig2 else 8
    page_width = round(width * 72 / dpi)
    page_height = round(height * 72 / dpi)
    content = f"q {page_width} 0 0 {page_height} 0 0 cm /Image Do Q\n".encode()
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        (f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width} {page_height}] "
         "/Resources << /XObject << /Image 4 0 R >> >> /Contents 5 0 R >>").encode(),
        (f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} "
         f"/ColorSpace /DeviceGray /BitsPerComponent {bits} /Filter /{pdf_filter} /Length {len(compressed)}>>\n"
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

    hp = sub.add_parser("hp-measure", help="measure the HP Stream corpus")
    hp.add_argument("--corpus", type=Path, required=True)
    hp.add_argument("--pdf-dir", type=Path, help="add page one of each local PDF in this directory")
    mode = hp.add_mutually_exclusive_group()
    mode.add_argument("--quick", action="store_true", help="target 10 minutes")
    mode.add_argument("--full", action="store_true", help="target one hour (default)")
    hp.add_argument("--measure", choices=HP_MEASUREMENTS, action="append", help="select a measurement; repeatable")
    hp.add_argument("--verify-corpus", action="store_true", help="verify the corpus hashes and exit")
    hp.add_argument("--pdfium", type=Path, required=True)
    hp.add_argument("--pdftoppm", default="pdftoppm")
    hp.add_argument("--pdftotext", default="pdftotext")
    hp.add_argument("--tesseract", default="tesseract")
    hp.add_argument("--hayro", help="optional Hayro renderer")
    hp.add_argument("--model-dir", action="append", type=model_directory, default=[], metavar="TIER=PATH")
    hp.add_argument("--trials", type=int, default=2, help="warm repetitions per case")
    hp.add_argument("--timeout", type=int, default=120, help="per-process limit in seconds")
    hp.add_argument("--memory-mib", type=int, default=1024, help="per-process address space limit")
    hp.add_argument("--output", type=Path, help="default: ~/hp-measure-YYYY-MM-DD.json")
    render = sub.add_parser("pdfium-render", help=argparse.SUPPRESS)
    render.add_argument("library", type=Path)
    render.add_argument("pdf", type=Path)
    render.add_argument("raster", type=Path)
    render.add_argument("page", type=int)
    render.add_argument("dpi", type=int)
    render.add_argument("format", choices=("gray", "rgb"))
    sample = sub.add_parser("pdfium-sample", help=argparse.SUPPRESS)
    sample.add_argument("library", type=Path)
    sample.add_argument("pdf", type=Path)
    sample.add_argument("page", type=int)
    sample.add_argument("format", choices=("gray", "bgra", "text"))
    sample.add_argument("trials", type=int)
    fixture = sub.add_parser("fixtures", help="make small scanned-page fixtures from Unifont")
    fixture.add_argument("--unifont", type=Path, required=True)
    fixture.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "hp-measure":
        if not (1 <= args.trials <= 20 and 128 <= args.memory_mib <= 4096 and args.timeout > 0):
            parser.error("check the trial, memory and timeout values")
        needs_ocr = not args.measure or {"ocr", "sustained"}.intersection(args.measure)
        if needs_ocr and not args.verify_corpus and set(dict(args.model_dir)) != set(HP_MODELS):
            parser.error("supply --model-dir for fast, standard and best")
        if args.pdf_dir:
            args.pdf_dir = args.pdf_dir.resolve()
            if not args.pdf_dir.is_dir():
                parser.error("--pdf-dir must be a directory")
        args.corpus = args.corpus.resolve()
    return args


def model_directory(value):
    tier, _, directory = value.partition("=")
    if tier not in HP_MODELS or not directory:
        raise argparse.ArgumentTypeError("expected fast=PATH, standard=PATH or best=PATH")
    return tier, Path(directory)


def main():
    args = parse_args()
    if args.command == "pdfium-render":
        pdfium_render(args.library, args.pdf, args.raster, args.page, args.dpi, args.format == "gray")
    elif args.command == "pdfium-sample":
        pdfium_sample(args)
    elif args.command == "hp-measure":
        return hp_run(args)
    elif args.command == "fixtures":
        fixtures(args.unifont, args.output)
    else:
        if not (1 <= args.trials <= 20 and 30 <= args.render_dpi <= 600 and args.timeout > 0
                and 128 <= args.memory_mib <= 4096 and args.english_page > 0
                and args.chinese_page > 0):
            raise ValueError("Check the trial, page, DPI and timeout values")
        run(args)


if __name__ == "__main__":
    sys.exit(main())
