#!/usr/bin/env python3
"""Generate the public HP Stream corpus from original text and pinned build tools."""

import argparse
import json
from pathlib import Path
import random
import runpy
import subprocess
import tempfile
import time

from PIL import Image


CONTENT = {
    "paper": {
        "title": "A field study of the North Quay tide marks",
        "author": "Clara Bell and Elias Ward",
        "left": (
            "The harbour office kept a ledger of the highest water on each working day. "
            "We compared five years of entries with the brass marks on the quay. "
            "On clear mornings the assistant aligned a field glass with the lower notch. "
            "The ledger gives a useful baseline, although the entries signed André differ "
            "in the precision of their figures. A second observer checked each transcription."
        ),
        "note": "An entry without a time was left out of the afternoon series.",
        "right": (
            "The first series showed little change. In the second series the flood reached "
            "the lower ofﬁce window twice. Fine sand filled the old channels after each "
            "storm. We marked the difference in blue ink and filed the original figures. "
            "The café near the slip kept a separate list of the days the ferry did not sail. "
            "These records agree most closely in the months when the wind was moderate."
        ),
    },
    "report": {
        "title": "North Quay maintenance report",
        "opening": (
            "This report records the autumn inspection of the quay. The team checked each "
            "location with the caretaker and assigned repairs for the next dry week."
        ),
        "headers": ["Location", "Priority", "Work ordered"],
        "rows": [["West steps", "High", "Replace the lower tread"],
                 ["Ferry shelter", "Normal", "Paint both doors"],
                 ["Harbour office", "Normal", "Seal the window frame"],
                 ["North railing", "Low", "Check the fasteners"]],
        "closing": (
            "The office will send a receipt when the timber arrives. On Friday, check "
            "the finished steps with the caretaker and file the receipt in the blue folder."
        ),
    },
    "book": {
        "title": "Leaves from a harbour notebook",
        "body": [
            "Before daylight the wind came down the river and shook the loose pane in my "
            "room. I put on my coat and walked as far as the old ferry stairs. A lamp burned "
            "in the harbour office, and beyond it I could make out the low decks of the "
            "packet boats. They rose and fell together in the dark water.",
            "At the foot of the lane a woman had opened her shop. She set a loaf on the "
            "counter and told me that the bridge was still closed. I bought bread for the "
            "journey and stood a little while beneath her awning. Across the street the "
            "rain ran off the roof in a single shining thread.",
            "When the tide turned I took the upper road. Under the elms the ground was dry "
            "enough to sit on. I opened my notebook and wrote down the names of the boats, "
            "the hour of the tide, and the few words I had heard in the shop. I have kept "
            "those particulars because they bring back the morning better than any account "
            "I could write now.",
            "Late in the afternoon I heard the ferry bell. It sounded twice, then a third "
            "time after a pause. The keeper put out the lamp, locked the door, and joined "
            "me on the road. We walked home along the bank without meeting another soul.",
        ],
    },
    "chinese": ["清晨的港口十分安静。", "渡船经过石桥，河面泛起微光。",
                "我在窗边记下潮水的时间。", "午后，图书馆的门向读者敞开。"],
    "vertical": ["河风吹过石桥", "船灯照着小路", "书页留住晨光"],
}


def transcripts():
    paper, report, book = (CONTENT[document] for document in ("paper", "report", "book"))
    return {
        "paper": [paper["title"], paper["author"], "Observations", paper["left"] + "1",
                  "Comparison", paper["right"], "1 " + paper["note"]],
        "report": [report["title"], report["opening"], *report["headers"],
                   *(cell for row in report["rows"] for cell in row), "Next steps", report["closing"]],
        "book": [book["title"], "Leaf 1", *book["body"]],
        "chinese": CONTENT["chinese"],
        "vertical": CONTENT["vertical"],
    }


def scan(source, name, seed, work, output, write_image_pdf, jbig2):
    raster, tiff = work / "page.pgm", work / "page.tiff"
    subprocess.run(["gs", "-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pgmraw", "-r300",
                    "-dFirstPage=1", "-dLastPage=1", f"-sOutputFile={raster}", source], check=True)
    with Image.open(raster) as page:
        image = page.rotate(0.55, resample=Image.Resampling.BICUBIC, fillcolor=255)
    width, height = image.size
    rng = random.Random(seed)
    for _ in range(width * height // 2200):
        x, y = rng.randrange(width), rng.randrange(height)
        image.putpixel((x, y), min(image.getpixel((x, y)), rng.randrange(65, 210)))
    write_image_pdf(output / f"{name}-scan-gray.pdf", width, height, image.tobytes(), dpi=300)

    bilevel = image.point(lambda pixel: 0 if pixel < 170 else 255, mode="1")
    bilevel.save(tiff, compression="group4", dpi=(300, 300))
    with Image.open(tiff) as page:
        stamp = time.gmtime(0)
        g4 = output / f"{name}-scan-g4.pdf"
        page.save(g4, dpi=(300, 300), resolution=300.0, creationDate=stamp, modDate=stamp)
    if b"/CCITTFaxDecode" not in g4.read_bytes():
        raise ValueError("Pillow did not encode the image as CCITT Group 4")

    if jbig2:
        png = work / "page.png"
        bilevel.save(png)
        encoded = subprocess.run([jbig2, "-p", png], check=True, stdout=subprocess.PIPE).stdout
        if not encoded:
            raise ValueError("JBIG2 encoder returned an empty stream")
        write_image_pdf(output / f"{name}-scan-jbig2.pdf", width, height, encoded, dpi=300, jbig2=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bench", type=Path, required=True)
    parser.add_argument("--font-path", action="append", required=True)
    parser.add_argument("--jbig2")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True)
    bench = runpy.run_path(str(args.bench))
    cases = []
    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory)
        template = work / "corpus.typ"
        template.write_bytes(Path(__file__).with_name("corpus.typ").read_bytes())
        (work / "content.json").write_text(json.dumps(CONTENT, ensure_ascii=False), encoding="utf-8")
        compiler = ["typst", "compile", "--creation-timestamp", "0", "--ignore-system-fonts"]
        for font_path in args.font_path:
            compiler += ["--font-path", font_path]
        for seed, (name, lines) in enumerate(transcripts().items(), 1):
            pdf, truth = f"{name}.pdf", f"{name}.txt"
            (output / truth).write_text("\n".join(lines) + "\n", encoding="utf-8")
            subprocess.run([*compiler, "--input", f"kind={name}", template, output / pdf], check=True)
            case = {"pdf": pdf, "truth": truth}
            if name == "paper":
                case["smoke"] = True
            if name == "book":
                case["pages"] = [1, 150, 300]
            if name in ("chinese", "vertical"):
                case.update(language="Chinese", ocr=True, psm=5 if name == "vertical" else 6,
                            model="chi_sim_vert" if name == "vertical" else "chi_sim")
            cases.append(case)
            if name not in ("chinese", "vertical"):
                scan(output / pdf, name, seed, work, output, bench["write_image_pdf"], args.jbig2)
                for codec in ("gray", "g4", "jbig2") if args.jbig2 else ("gray", "g4"):
                    cases.append({"pdf": f"{name}-scan-{codec}.pdf", "truth": truth,
                                  "codec": codec, "ocr": codec == "gray"})
    manifest = {
        "schema": 1,
        "scan_dpi": 300,
        "jbig2": "generated" if args.jbig2 else "encoder unavailable in pinned nixpkgs",
        "cases": cases,
        "sha256": {path.name: bench["file_hash"](path) for path in sorted(output.iterdir())},
    }
    (output / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
