# Reviewed draft and selective passage restore

Draft comparisons now retain the exact saved snapshot ID, content, label and
origin selected in the milestone/variant list. Previously the comparison route
had a separate navigation selection, usually zero; restoring a second saved
draft could therefore restore the first. A real compiled-app reproduction
confirmed that failure before this fix. Both the visible summary and restore
now use the captured reviewed version.

In **Draft milestones**, select a named milestone or active-scene variant and
press Enter. `r` restores the complete reviewed scene, preserving the existing
recovery variant, dated history and normal editor Undo. For one changed passage,
use Left/Right or `[`/`]`, then `p`. A shared menu defaults to Cancel above the
complete current/saved wording panel; Page Up/Down scrolls. Enter on Cancel or
Esc keeps the writing. Explicitly select Restore before Enter to replace only
the chosen changed group. Other edits, private notes and original YAML remain.

This reuses the existing `history::diff` and editor; no comparison engine,
parallel manuscript or durable byte-index store is added. Adjacent changed word
runs form one temporary passage, separated by retained text. The adapter verifies
that the diff reconstructs the entire current UTF-8 body before replacing any
range. Bounds are 2 MiB per compared body, 16,384 runs and 4,096 groups. Exceeding
a bound refuses the comparison in full. Existing editor-ownership/review limits
and the 1-MiB guarded milestone/variant sidecar bounds still apply.

Application rechecks document identity, editor epoch/revision, source ownership,
backing-file fingerprint, guarded metadata and the reviewed in-memory metadata
revision before applying. It saves the full current body as a recovery variant
and writes existing dated history before changing the retained editor. A failed
guard or recovery/history write leaves prose untouched. A recovery variant may
remain after a later history failure; it is retained rather than rolled back.
Schema-1 prototypes, unknown/future/corrupt stores and external changes remain
preserved. Normal autosave, save conflicts and Undo continue to own manuscript
persistence.

Offline help and lesson 33 explain the workflow; its existing HistoryOpened
event remains a history/comparison exercise, not proof of passage restoration.
No new credit is granted by viewing a confirmation or cancelling.

Focused core tests cover independent edits, Unicode, empty/inserted/deleted/EOF
changes, malformed/stale diffs and bounds. Four-size UI and actual compiled-app
checks cover second milestone/variant selection, default Cancel, explicit
passage restore, help, hidden-input protection, retained unrelated writing,
recovery/history, Undo and fresh-process restart. Stale source, backing and
external/in-process metadata refusal are separately tested. Full source-bound
Debian qualification is in progress; this does not complete character knowledge,
all M8/M9, large-load/power-loss or production OS qualification.
