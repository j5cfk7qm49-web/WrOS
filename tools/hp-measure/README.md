# HP Stream PDF and OCR measurements

Build the kit on the review machine and send its closure over the side SSH
connection. Substitute the SSH alias or user and address already set up for
the laptop. Use an account accepted by its Nix store for the copy, then run the
measurement as the owner. Keep the build link until the measurements finish.

```sh
nix build .#hp-measure --out-link result-hp-measure
kit=$(readlink -f result-hp-measure)
nix copy --to ssh://wros-side "$kit"
ssh wros-side "$kit/bin/wros-hp-measure --quick --output hp-measure-quick.json"
ssh wros-side "$kit/bin/wros-hp-measure --full"
```

The runner uses the copied store paths and works offline. Run it with the Nix
store and home directory on the laptop's eMMC. The quick pass targets 10 minutes.
The default full pass targets 60 minutes and includes 30 minutes of sustained
OCR. The timed windows use real OCR work. Runtime varies with the available
models, selected measurements, and process timeouts. Both modes write one
`~/hp-measure-YYYY-MM-DD.json`. A second run on the same day replaces that file.
Set `--output` to keep both runs. The report stays on the laptop until the owner
copies it out.

From a shell on the laptop, run the copied wrapper with a folder of PDFs already
on the laptop. The wrapper accepts a top-level folder, reads page one of each
PDF, and includes its SHA-256 in the JSON. It records no owner filenames,
absolute paths, or recognized text. The original files stay in place, and the
runner removes temporary raster and text output when it exits normally.

```sh
"$kit/bin/wros-hp-measure" --quick --pdf-dir "$HOME/Documents/Benchmark PDFs"
"$kit/bin/wros-hp-measure" --measure start --measure extract --output "$HOME/hp-text.json"
"$kit/bin/wros-hp-measure" --verify-corpus
```

Use `--measure` to select one or more measurements and run only that selection.
An explicit `--quick --measure sustained` uses a 60-second thermal sample.
The public corpus includes a two-column paper, a report, a 300-page book, and
horizontal and vertical Simplified Chinese. Each of the three English source
pages has a 300 dpi grayscale scan and a CCITT Group 4 scan. The builder also
generates JBIG2 scans when the pinned Nix package set provides `jbig2enc`. Check
`share/hp-measure/corpus/MANIFEST.json` for the hashes and encoder status.

## Research measurement record

| # | Selection | What it measures |
|---|---|---|
| 1 | `hardware` | CPU model and flags, RAM, kernel, eMMC devices, and the backing devices for the corpus and report. |
| 2 | `smoke` | PDFium build path and hash, successful library binding, and a render and extraction of page one of the paper. The binding uses ctypes. |
| 3 | `start` | Fresh PDFium helper wall time, dynamic library load, document and page open, and first-page render time. |
| 4 | `render` | Fit-width render time and process peak RSS for PDFium grayscale and BGRA, and Poppler and Hayro grayscale and RGB. |
| 5 | `extract` | PDFium and Poppler text extraction time, peak RSS, character count, and normalized character error rate on transcribed text pages. |
| 6 | Manual | Native composition, blit, and page-flip latency with highlights and overlays require the reader UI. |
| 7 | `ocr` | Single-worker Tesseract time, process peak RSS, and character error rate at 200 and 300 dpi, using fast, standard and best English models and packaged Simplified Chinese models. |
| 8 | `sustained` | Repeated 300 dpi English fast OCR for 30 minutes with throughput, peak RSS, and thermal-zone and CPU clock samples every 10 seconds. Foreground input latency requires a separate test. |
| 9 | Separate | ONNX instruction compatibility and PP-OCRv5 performance and accuracy require an ONNX benchmark. |
| 10 | Separate | ocrs English page time, peak RSS and accuracy require an ocrs benchmark. |
| 11 | Manual | Unifont Hanzi width, wrapping and selection require an interactive reflow view. |
| 12 | Manual | Foot sixel encoding, presentation time and memory require a sixel-capable session. |

The kit covers the packaged PDFium, Poppler, Hayro render and Tesseract baselines.
Hayro and pdf_oxide extraction, the Rust PDFium binding check, Traditional
Chinese and two-worker OCR remain separate comparisons. PDFium warm render
timings reuse an open page. Hayro and Poppler warm timings use fresh CLI
processes against a warmed input and include writing the raster. Hayro uses
the nearest whole DPI to fit the requested width. Cold trials request eviction
of the specific PDF and PDFium library, or the OCR raster and traineddata, from
the file cache. The kernel can retain cached pages. OCR timings exclude
rasterization, which runs once per page and DPI. The vertical page uses upright
glyphs laid out in columns with a right-to-left reading order. Its transcript
follows that order, so extraction order affects the error rate.
