# Notes & TKs

Leave yourself notes in the middle of the prose. They stay in the scene's file but **never reach the book**.

- Anything between `%%` marks is a note: `%% check the date of the siege %%`. A note can run across several lines.
- `TK`, on its own and in capitals, is the editor's mark for "to come": *she took the TK train north*.

Notes are shown dimmed in italics and TKs in bold. Neither is counted in any word count, and notes are never compiled or exported. Obsidian treats `%% … %%` as a hidden comment too.

## Finding them again

- `Ctrl-T` lists every note and TK in the book. Type to filter the list; `Enter` goes to one.
- *Next TK* in `Ctrl-K` jumps to the next one after the cursor.

## TKs when you export

A TK is still in the text when you export, so it's never silently dropped from the middle of a sentence. Export lists every TK first and waits: press export again to go ahead anyway, or go and fill them in.

## Footnotes · browse and jump

Footnotes are **public manuscript content**, unlike private `%% ... %%` notes. Write a reference such as `[^source]` in the prose and define it in the same scene with `[^source]: Author and page.`. Use **Writing tools → Footnote navigator…** or **Ctrl-K → Footnotes · browse and jump** to inspect references, see previews and warnings for missing, unused or duplicate definitions, and jump to the original Markdown.

To write a new footnote, put the cursor in an editable scene and choose **Writing tools → Insert footnote here** or **Ctrl-K → Insert a Markdown footnote at the cursor**. WrOS inserts a unique `[^fn-1]`-style reference, appends the matching `[^fn-1]: ` definition in the same scene, and positions the caret at the definition so you can type immediately. Existing note labels and text are preserved; undo restores the original before insertion. You must clear an active selection first, and code/private-note positions are refused.

In the navigator, **Enter** selects the original reference and **d** moves to the definition for ordinary editing with Undo, autosave and history. Missing or duplicate definitions refuse the definition jump. A changed, missing or unresolved source refuses stale navigation: return to writing, reconcile the source and reopen the navigator. F1 opens this help; paste, cut and undo cannot alter hidden prose while the navigator or Help is open. The browser limits inspection to 2 MiB per scene, 16 MiB total, 8192 candidate markers per scene and 2048 listed items with 256-character previews; exceeding a limit preserves the manuscript and reports it.

The manuscript **Word** export supports real page footnotes; **PDF** uses the
same DOCX through headless LibreOffice. **EPUB** supports linked notes with
return links. In Export, untick Paperback when using footnotes; that edition
still refuses footnoted scenes.

Manuscript notes support text paragraphs, emphasis, code and readable link
targets. Each reference receives a consecutive numbered note; repeated labels
repeat their definition at each reference. Definitions remain local to their
scene. Missing, duplicate, uncited or nested notes refuse the rich export.
Lists, images, tables and raw HTML in a footnoted scene also refuse it until
their formatting is supported. Markdown-only export preserves the source.

Limits are 2 MiB and 8,192 markers per footnoted scene, 64 KiB per definition,
4,096 document notes and 16 MiB rendered note content. Refusal occurs before
creating companion export files. Your original Markdown/YAML stays unchanged.


## Read the footnote at your caret

Place the caret on a public reference or inside a definition, then choose
**Read footnote at caret** in Ctrl-K or Writing tools. In the navigator,
**v** reads the selected note and Esc returns to the same browser selection.
The shared panel sits below your original writing and shows the complete
public definition as source Markdown. Private notes are omitted from this
reader; they remain intact in the original editor above it.

**Up/Down** or **Page Up/Page Down** scroll; **Home/End** reach the start/end.
**Left/Right** chooses a reference when the same note is cited more than once.
**r** selects that original reference; **d** or Enter moves to the original
definition for normal editing, autosave, history and Undo. Reopen at the
edited definition to read its current content, then r returns to a reference.
The preview never edits or replaces your source. F1 opens this offline help.
Paste, cut, undo and dragging do not affect hidden writing while reading.

This is a source snapshot. A changed backing file or editor revision refuses
navigation: close, reconcile using the ordinary editor, and reopen. Missing
and duplicate definitions are shown honestly and cannot choose a definition;
an unused definition has no reference to jump to. Inspection is bounded to
2 MiB per scene, 8192 candidate markers and a 64 KiB definition snapshot.
An exceeded bound refuses the reader and keeps the original source available.
This contextual reader does not render printed pages or qualify rich exports.
Lesson 34 completes only after a real manuscript Word export with notes and
its matching successful PDF conversion in the safe demo. Viewing this reader
or exporting Word alone does not complete it.
