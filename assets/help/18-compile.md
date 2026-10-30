# Compile: the manuscript

When the book is ready to send, *Export…* in the menu (*Export for readers* in `Ctrl-K`) compiles it into a manuscript in **William Shunn's modern standard manuscript format** — the one agents and editors expect.

## What you get

- **A title page**: your contact block top left, "about 82,000 words" top right (rounded the way publishers expect), the title halfway down, and your byline under it.
- **Every page after it**: Times New Roman 12, double-spaced, one-inch margins, half-inch indents, and `Surname / KEYWORD / page` in the top right corner.
- **Chapters** start a third of the way down a new page; a part shares its first chapter's page. A Prologue or Epilogue keeps its own heading.
- **Scene breaks** are a centred `#`, and the manuscript ends with END.
- **Typography**: curly quotes and apostrophes, real dashes and ellipses. Your own files stay exactly as you typed them.
- **Nothing half-finished goes out quietly**: empty scenes and chapters are left out, `%% notes %%` are removed, and every TK is listed before anything is written.

## The export dialog

- `↑` `↓` move between rows. `Space` or `Enter` ticks a format or a part.
- **Formats**: Word manuscript, PDF, Paperback, EPUB and Markdown.
- **Include**: which parts go in.
- **How much**: `←` `→` choose the whole book, the first chapters, chapters X–Y, or the first N words — the samples agents ask for — and type the number or range. The title page still gives the whole book's word count.
- **Look…** and **Author details…** (below).
- `p` reviews the loaded source selected by these formats, parts and sample.
  It lists missing, duplicate and unused footnotes, public TK/TODO/FIXME
  markers and empty-source notices. `↑`/`↓`, Page Up/Down and Home/End scroll;
  `Esc` returns to the same export settings. Reopen the review to refresh it.
  It also reports when the current rich-format selection is blocked by the
  paperback footnote-export guard; deselect Paperback for supported manuscript
  Word/PDF and EPUB notes, or use Markdown to preserve the syntax. Rich export
  validates renderable note structure when exporting; this review does not render pages.
  It does not write export files or check links, continuity, revision notes,
  rendered pages or externally changed files. It inspects at most 512 scenes,
  16 MiB total, 2 MiB and 8,192 footnote markers per scene, and 512 findings; an exceeded limit
  refuses the review instead of displaying a partial report.
- `x` (or `Enter` on the Export button) exports into the book's `exports/` folder. Files are named the way agents ask: `Surname_Title_Manuscript.docx`.

## Look…

The manuscript's format, also in *Esc › Settings › Manuscript look…*: **Modern** (Times) or **Classic** (Courier, underlined italics, straight quotes), US Letter or A4, double or 1.5 spacing, the chapter heading style (Chapter One, CHAPTER ONE, Chapter 1, 1, or the chapter's title), first paragraphs indented or flush, END / THE END / nothing, how a spaced hyphen is set, and the header keyword. `↑` `↓` choose, `←` `→` change, `Esc` saves.

## Author details…

Your byline (pen name), legal name, address, phone, email and an agent's details if you have one — for the title page. Saved in the book's `novel.toml`. Until they're filled in, the export dialog says so.

## PDF

A PDF is made with **LibreOffice** (free, from libreoffice.org). Without it, export makes the Word file and says how to get LibreOffice; Word can also save the file as a PDF.

From the command line: `grimoire export --docx --pdf --chapters 1-3` (or `--words 10000`, `--parts 1,3`, `--epub`, `--md`).

## EPUB footnotes

EPUB exports public scene footnotes as numbered links with a return link in
each note. Definitions stay local to their scene, including front/back matter;
repeated references repeat the definition. Note paragraphs, emphasis and code
are retained. Link targets appear as exact readable text. No resource is fetched.
Missing, duplicate, uncited, empty or nested notes and unsupported lists, images,
tables or raw HTML refuse export before any companion files are written.
Your original Markdown/YAML and Undo are preserved. The limits match the
manuscript notes documented in **Notes and TKs**. Paperback notes remain guarded.
EPUB-only output does not complete the Word/PDF exercise in lesson 34.
