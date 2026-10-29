# Revision notes

Select a passage in your existing editor. Open **Revision notes** from the
command palette, Writing tools, or Settings > Writing. Press **n**, type your
comment, then **Enter**. **Esc** cancels a draft. Notes remain local, outside
the Markdown/YAML manuscript, and do not enter publishing exports.

The queue lists open and in-progress notes across this project. **f** switches
between Open and All, including resolved notes. **Up/Down** chooses a note and
**Enter** reviews it. The source path is shown with each note. Open that source
in the ordinary editor before reviewing its anchor. Closing the queue retains
the active pane, caret, selection, scroll and Undo. This first slice does not
automatically switch documents from the queue.

## Review a passage

| Key | Action |
| --- | --- |
| g | Select the exact, uniquely matched passage in the active editor |
| e | Edit the comment; saving reopens the note |
| s | Mark In Progress |
| x | Resolve against this reviewed source revision |
| o | Reopen against the current matched passage |
| a | Review a replacement anchor from your current editor selection |
| r | Refresh the source in Review; explicitly reload metadata in Queue |
| Page Up / Page Down | Scroll long passages and comments |
| Esc | Back to the queue, then the unchanged editor |
| F1 | This offline help |

Use normal editor actions to revise the manuscript, with its existing Undo,
autosave, recovery and history. Annotation actions never replace prose.
Resolution is a personal review decision, not automated writing advice.
If the source changes after resolution, Review warns that it was resolved on
an older revision. The note stays visible under All.

## Changed or ambiguous anchors

Anchors use exact original UTF-8 text and up to 32 characters of context on
each side. Offsets cannot break ties. Moving a passage with its context is
supported. Changing the quote or context, or duplicating the passage, can make
an anchor unavailable. No nearest-match guess changes or selects prose.

Return to the editor, select the intended new wording, reopen your note, and
press **a**. Review both passages, select **Save reviewed anchor** with Down,
then Enter. Cancel is the default. The note reopens and retains its old anchor.
Lesson 32 in the safe demo verifies a real changed-anchor save.

## Document identity and preservation

Each annotated document receives a durable number in the project sidecar.
Its relative path is a locator, never a project-tree index or scene title.
Adding unrelated scenes and moving the entire project do not change this ID.
After a file or folder rename, open the new source and explicitly re-anchor
one of its notes. If the old path is missing and the new path is not already
bound, confirmation reconnects the same document ID and all its notes. Other
notes still require their own exact quote/context matches. Existing paths and
identities cannot be merged. Renumbering that reuses an old path or external
replacement with a different document requires manual metadata recovery;
automatic rename/reorder reconciliation is not provided by this slice.

Metadata lives at `.grimoire/writingdeck/review-notes.json`. IDs are never
reused. Earlier comments, anchors and review statuses are retained for up to
64 changes per note; reaching that limit refuses a new change rather than
discarding history. This is retained metadata, not a history-restore UI or
an independent backup. Comments are limited to 4096 bytes and selections to
16 KiB; the complete metadata store has the existing 1 MiB bound.

Unknown, corrupt, oversized, externally changed or aliased metadata is
preserved. Failed writes keep the draft available to copy or retry. Queue
reload is explicit and should be used only after cancelling the draft.
Prototype annotations with tree-index identities are preserved and require
an explicit migration; they are never silently imported or reassigned.
These guards do not claim cross-process locking or a multi-file transaction.
