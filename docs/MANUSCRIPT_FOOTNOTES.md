# Manuscript Word/PDF page footnotes

The existing manuscript exporter now uses the pinned pulldown-cmark parser to
place public Markdown notes in a real Word footnotes part. The existing
LibreOffice adapter produces the PDF. Original Markdown/YAML is never rewritten.
No external links, images or manuscript content are fetched or transmitted.

In **Export for readers**, choose Word and/or PDF and turn off EPUB and
Paperback for a footnoted manuscript. Press **p** for selected-source review,
**Esc** to return and **x** to export. A PDF includes its Word source. Wait for
the actual PDF-ready result before leaving the application. Another export
waits while the current PDF conversion is running.

Each reference gets a consecutive note number. Repeated references repeat the
definition at the new occurrence; identical labels in different scenes are
independent. Paragraphs, multiple note paragraphs, long-note continuation,
emphasis, strong text, code, headings, blockquotes and readable link targets are
supported. Classic manuscript emphasis retains its underline convention.
Links are readable text with their targets, not active external relationships.
Scenes without public footnotes retain the existing native formatting path.

Missing, duplicate, uncited, empty or nested notes and unsupported structures
such as lists, images or raw HTML in footnoted scenes refuse before any output
directory or companion format is written. EPUB/paperback notes remain guarded.
Markdown-only export remains available. Bounds are 2 MiB and 8192 markers per
scene, 64 KiB per definition, 4096 references and 16 MiB of rendered note XML
per document. These are bounded manuscript notes, not full Markdown publishing
or a promise of identical pagination in every Word-compatible application.

Lesson 34 opens the actual export workflow. Completion requires a safe demo,
a successfully serialized Word manuscript containing page-note references and
its matching nonempty PDF after LibreOffice succeeds. Viewing, Word alone,
failed exports, or merely starting conversion cannot complete it. Existing
tutorial history is preserved.

The Debian renderer checks use LibreOffice Writer, Poppler and Liberation
fonts. Modern/Letter/double and Classic/A4/1.5 fixtures check same-page bottom
placement, independent/repeated labels, Unicode, private-note exclusion and
unchanged original bytes. A long note must begin beside its reference and
continue across actual pages. Four-size compiled TUI checks exercise source
editing, refusal, preflight, real export, lesson progress, Undo and restart.
Focused checks passed; full source-bound Debian qualification is in progress.
This does not refresh the OS image, qualify physical hardware or complete M9.

The package follows the documented [Word footnotes part](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnotes?view=openxml-3.0.1),
[body reference](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnotereference?view=openxml-3.0.1)
and [page-bottom position](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.footnoteposition?view=openxml-2.8.1)
contracts. Package assertions and actual rendered pages establish separate
things; both are required here.
