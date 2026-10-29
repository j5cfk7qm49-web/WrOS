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

True bottom-of-page footnotes in Word/PDF and EPUB note links are **not yet supported**. Choose Markdown-only export for footnoted manuscripts; rich-format export refuses those documents rather than misrepresenting their notes.
