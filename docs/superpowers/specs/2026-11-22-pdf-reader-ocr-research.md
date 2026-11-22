# PDF reader and OCR for WrOS

The research draft and four underlying notes support PDFium for the page reader and Tesseract for local OCR. This synthesis records the choices, their limits and what the HP Stream must still prove. It also feeds the [reader design](2026-11-22-brain-library-review-design.md#reader-stage-4). No timings below were measured on that laptop.

## Rendering and extraction

| Option | What it gives WrOS | Decision |
|---|---|---|
| PDFium via pdfium-render | Character Unicode, boxes, positions, rotation and mapping-error information, plus page rasterization and selection rectangles | Use in a small helper with pinned Rust bindings and a matching Nix library. |
| hayro | Permissively licensed Rust renderer. Text extraction requires more work and has unresolved Unicode cases in the researched release. | Benchmark as a fallback candidate. |
| pdf_oxide | Permissively licensed character extraction and reading-order options | Compare extraction results. |
| Poppler utilities | Mature raster and text output | Run as separate benchmark/debug processes only. |

Prefer PDFium because selection, reflow and durable highlights need characters tied to page coordinates. A bounded helper owns PDFium and communicates over a narrow pipe protocol. Give it memory, output and per-page time limits, and restart it after failure. Pass the exact Nix library path and smoke-test the symbols in the packaged build. A cargo build alone cannot prove the dynamic library matches the binding. [PDFium text API](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h) · [pdfium-render](https://github.com/ajrcarey/pdfium-render) · [hayro](https://github.com/LaurenzV/hayro) · [pdf_oxide](https://github.com/yfedoseev/pdf_oxide)

Composite the page bitmap, selection and menus before native DRM presentation. Cover resize, VT recovery, keyboard pan/zoom and a text fallback. A 1,366 × 1,768 page takes about 9.7 MB RGBA or 2.4 MB grayscale before caches and renderer overhead. Budget RAM and disk in bytes, prefetch nearby pages, and render visible tiles at high zoom. Keep console, wgpu and Cage/Foot usable as text first. Measure Foot sixel before adding that path. Limit linked code to compatible licences. Keep Poppler as a debug subprocess, with MuPDF permitted only as a separate research cross-check under the same licensing rule.

## Text and durable highlights

Keep a versioned canonical text layer in PDFium character order, excluding out-of-crop characters and recording generated characters separately. Normalize Unicode, whitespace and soft hyphens with an offset-to-character-and-box map. Where a visual word gap needs a synthesized separator, record its adjacent characters. Keep reading order as a separate projection. Prefer valid structure tags, then geometry-based lines, blocks and column cuts. Repeated margin text can be skipped during reflow. Preserve footnotes and use placeholders that jump to figures, tables and equations. Exercise ligatures, multi-column papers, CID fonts, vertical Chinese and wide glyphs. [PDFMiner layout analysis](https://pdfminersix.readthedocs.io/en/latest/topic/converting_pdf_to_text.html) · [PDFium text API](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h)

Persist a document fingerprint, page index and printed label, page size and rotation, rectangles in unrotated PDF space, normalized exact quote with surrounding context, and page-relative code-point range. Record anchor, normalization and text-layer versions. Use SHA-256 for exact identity. Optional trailer IDs, page count, geometry, text hashes and image hashes help classify changed files. Keep large comparison manifests and caches outside Loro. Anchors retain the small identifying signals needed to detect a missing or ambiguous target. [W3C selectors](https://www.w3.org/TR/annotation-model/) · [Hypothesis PDF anchoring](https://github.com/hypothesis/client/blob/main/src/annotator/anchoring/pdf.ts)

Keep the stored rectangles for an unchanged file. Where geometry and visual evidence confirm the same pages with a changed OCR layer, re-read the quote from the old rectangles and flag material mismatch. Where content or pagination changes, look for a unique match using quote, context and page alignment. Require both a minimum score and a margin over competing matches. Show uncertain matches as orphaned with the old quote and a way to reattach. Store successful changes as a new anchor revision. Region highlights on scans are usable before OCR. WrOS never needs to rewrite the original PDF to attach them.

## OCR and Chinese

Use Tesseract CLI with pinned language models. Start with English and Simplified/Traditional Chinese, including vertical models for appropriate pages. Isolate the parser, cap jobs and give each worker one thread. Prefer one low-priority worker. Permit two while idle on AC only if thermal and foreground latency checks pass. Promote the current page and the next three. Persist a page only after its sidecar is complete and validated, and resume by checking which sidecars are missing. Cancel or pause promptly and keep the original file intact. [Tesseract CLI and language selection](https://tesseract-ocr.github.io/tessdoc/Command-Line-Usage.html)

Store per-page hOCR or TSV, normalized boxes and a manifest keyed by PDF hash, page, DPI, languages and engine/model versions. Sidecars, layout, tiles and queue state are rebuildable. Keep them local in the content-hash-keyed cache. Store the selected quote, rectangles, layer ID and annotation itself in Loro. A Windows PC can supply sidecars in the same validated format. Verify document identity, pixel-to-PDF transform and bounds on import. Higher-quality OCR that cannot be regenerated may be kept as an explicit export by the owner.

The draft's 1–2.5 hour estimate for a 300-page English scan is an extrapolation. Actual throughput, heat and language accuracy are open. Compare standard tessdata with fast and best models. Test PP-OCRv5 mobile through ONNX Runtime only after proving the installed binary runs on the HP Stream's CPU and fits the memory budget. Keep PC offload available for difficult Chinese scans. Test ocrs as an English-only comparison. [PaddleOCR models](https://www.paddleocr.ai/latest/en/version3.x/algorithm/PP-OCRv5/PP-OCRv5.html) · [ONNX Runtime builds](https://onnxruntime.ai/docs/build/inferencing.html) · [ocrs](https://github.com/robertknight/ocrs)

## HP Stream measurement record

Use LaTeX papers, Word PDFs, JBIG2 and CCITT scans, Chinese CID-font PDFs, and hand-checked horizontal and vertical English/Chinese pages. Record the exact files or content hashes, package versions, settings, elapsed time, peak RSS and relevant output checks. Run cold and warm cases on the actual eMMC. Stage 3 prepares the kit and records the measured values. Hardware results are pending.

| # | Measure on the HP Stream |
|---|---|
| 1 | CPU SKU, instruction flags, RAM and storage. |
| 2 | PDFium library build and matching Rust feature, successful binding and a packaged render/extract smoke test. |
| 3 | Cold helper start, dynamic library load and first-page time. |
| 4 | Warm fit-width PDFium, hayro and Poppler CLI render time and RSS, including grayscale and RGBA where offered. |
| 5 | Extractor time and correctness on accents, ligatures, columns, Chinese CID fonts and vertical text for PDFium, hayro and pdf_oxide. |
| 6 | Native composition, blit and page flip latency during scroll with highlights and overlays. |
| 7 | Tesseract seconds per page, whole-job peak RSS and character error rate at 200/300 dpi, for English and Simplified/Traditional Chinese, fast/best and vertical where relevant, with one and two workers. |
| 8 | Sustained 30-minute OCR, CPU temperature, clock and foreground input latency. |
| 9 | ONNX Runtime instruction compatibility, PP-OCRv5 mobile time and peak RSS, with accuracy alongside Tesseract. |
| 10 | ocrs English page time, peak RSS and accuracy. |
| 11 | Unifont wide Hanzi layout, selection and wrapping in reflowed cells. |
| 12 | Foot sixel fit-width encode/presentation time and memory. |

Build basic extraction first for selection and the console. The first complete PDF milestone adds native page images, marks and the full anchor schema. Add resumable OCR and geometry re-anchoring next. Add full reflow, then qualify text-based re-anchoring across file revisions and any optional display or OCR engine.
