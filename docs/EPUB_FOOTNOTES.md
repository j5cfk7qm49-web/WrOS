# Native EPUB footnotes

EPUB now uses the same bounded public-source footnote parser as manuscript
Word/PDF. The existing native EPUB package remains in use. The pinned maintained
`pulldown-cmark` HTML renderer handles parsed text and styles; WrOS adds numbered
note/reference links, same-file return links and publishing validation. Source
Markdown/YAML is never rewritten. Nonfootnoted scenes retain the existing path.

Each reference receives a unique consecutive note, including repeated references
to one definition. Labels stay local to a scene; front/back matter follow the
selected ebook edition. Multiple note paragraphs, Unicode, emphasis, headings,
quotes and literal code are preserved. Normal body lines retain Grimoire's
paragraph convention, including balanced emphasis across line boundaries. Link
targets appear as exact readable text, with no resource fetch or remote access.

The note uses `epub:type="footnote"` and `role="doc-footnote"`; reference and
return anchors pair EPUB semantics with DPUB-ARIA roles and accessible names.
These follow [W3C EPUB structural semantics](https://www.w3.org/TR/epub-ssv-11/#notes-and-annotations)
and [DPUB-ARIA](https://www.w3.org/TR/dpub-aria-1.1/#doc-backlink). This markup does
not establish compatibility with every reader or assistive technology.

Missing, duplicate, uncited, empty or nested notes refuse the export. Lists,
tables, images and raw HTML in footnoted scenes remain unsupported and refuse
before creating the output directory or changing Word/Markdown companions or
previous outputs. Bounds are 2 MiB and 8192 markers per scene, 64 KiB per
definition, 4096 notes per EPUB and 16 MiB rendered note content. Paperback
footnotes remain guarded. Markdown-only output retains footnote syntax.

The Export dialog, source preflight, footnote reader and offline help reflect
the available editions. Lesson 34 still requires real manuscript Word notes and
their matching completed PDF conversion; EPUB alone earns no completion.

Focused verification includes native XML/reference checks, invalid and oversized
source refusal before companions, actual compiled CLI output checked by Debian
EPUBCheck, and real export/help/edit/Undo/restart at four terminal sizes. Java and
EPUBCheck are build/test dependencies only. Invoke the packaged validator as
`java -Xmx256m -jar /usr/share/java/epubcheck.jar FILE`; host kernel binary handlers
are unnecessary. Debian EPUBCheck 4.2.6 checks EPUB 3.2 rules, which is narrower
than full reader, accessibility or production publishing qualification.

The [independent Debian run](evidence/epub-footnotes-local.json) passed
1065 Rust tests per configuration, 182 Python tests with zero skips, both Clippys and 21 gates. [Provenance and validator receipts](evidence/epub-footnotes-local-provenance.json) bind the exact tested source and binary. Existing Word/PDF
[page-footnote evidence](MANUSCRIPT_FOOTNOTES.md) remains separate.
