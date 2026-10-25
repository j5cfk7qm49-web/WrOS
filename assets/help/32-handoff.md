# Session handoff

Leave a next-session intention as ordinary original Markdown at notes/handoff.md. Open Session handoff from Ctrl-K, Writing tools or dashboard o. This is user-authored prose, not automatically generated advice, a word/time ledger or a backup.

| Key | Action |
| --- | --- |
| Enter / e | Explicit create/open original in Secondary / Primary |
| s | Save both source buffers, then arm this nonempty original for next book open |
| g | Explicitly resume the saved manuscript context |
| o | Toggle next-open display without changing prose |
| Up / Down, PgUp / PgDn, Home / End | Scroll the reader, not either editor |
| r | Explicit metadata/source reload |
| Esc | Continue writing |

Write your intention in the actual source editor and save it. Return to Session handoff and press s. The guarded workflow index records a timestamp, source fingerprint and optional manuscript path/caret/selection/scroll; prose remains only in the original Note. At the next book/app open the reader shows that original, unless a recovery dialog needs priority. Turning startup display off leaves both original and saved context intact. Missing/unavailable originals are reported, not silently replaced. Explicit g restores a context through existing source ownership and clamps editor positions; it does not overwrite a scene or create a duplicate writer.

A saved manuscript context is optional, not a guessed writing session: if both editors are notes there is no invented manuscript path. Timestamp means intention left, not typing duration or proof of process exit. Later edits remain ordinary source edits, not a frozen generated copy. Save/recovery, raw YAML and undo stay in the existing original editor. Strict focus never focuses hidden Secondary. Clipboard/bracketed paste in the reader is ignored; use the explicit editor. Unknown/future/conflicting workflow metadata is preserved; original prose is independent. The existing explicit force-quit instruction remains visible even if a journal/metadata warning is long.
