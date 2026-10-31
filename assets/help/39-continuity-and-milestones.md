# Continuity & draft milestones

Open **Continuity tracker**, **Draft milestones**, or **Read chapter aloud**
from the command palette (`Ctrl-K`) or **Writing tools**. All long-form
tracking data lives in `.grimoire/writingdeck/` outside your Markdown scenes
and never alters manuscript prose unless you explicitly restore a snapshot or
scene variant.

## Continuity tracker

The continuity tracker records explicit, author-entered facts, ordered
chronology events and plot threads without guessing or hallucinating conflicts
from prose.

- Press `1` for **Facts**, `2` for **Timeline**, `3` for **Plot Threads**, and
  `4` or `c` for **Contradictions & Suggestions**.
- Press `n` on Facts, Timeline or Threads to add a new record. Use `Tab` or
  `Up`/`Down` to move between fields, `Left`/`Right` to cycle category,
  certainty or thread status, and `Enter` to save.
- A **Definite Conflict** requires two **Definite (Canon)** facts on the same
  subject and property that share the same non-empty exclusive scope (for
  example, `canon`). Unscoped or changing facts produce a **Suggested
  Inconsistency** for review. Facts marked **Retconned / Obsolete** are
  excluded from conflict checks.
- Press `t` on a fact to cycle its certainty, or on a plot thread to cycle
  between Open, Resolved and Abandoned. Press `d` to remove the selected item.

## Draft milestones and scene variants

Named **Draft milestones** snapshot every compiled manuscript scene across your
book, while **Scene variants** store reversible alternative versions of the
active scene (such as a different point of view or ending).

- Press `1` for **Named Milestones** and `2` for **Active Scene Variants**.
- Press `n` to create a named milestone or save the active scene as a named
  variant.
- Press `Enter` on any milestone or variant to inspect its word-count delta and
  preview its text alongside your current scene. The comparison and restore
  stay bound to that exact reviewed snapshot, including the second or later
  entry in a list.
- Press `r` inside the comparison view to restore the snapshot or
  variant into the active editor. Before replacing the buffer, Grimoire
  saves the current wording as a recovery variant and in dated history. Failed
  recovery/history writes, changed source or changed metadata refuse the restore.
  `Ctrl-Z` undoes a successful restore in the original editor.
- To restore only one changed passage, use `Left`/`Right` (or `[`/`]`) in the
  comparison, then `p` to review the complete current and saved wording.
  `Page Up`/`Page Down` scroll. **Cancel is selected initially**; `Enter` keeps
  your current writing. Use `Down` or `Tab` to explicitly choose Restore,
  then `Enter` applies only that change. Other edits remain intact. A recovery
  variant and dated history are kept before the change; `Ctrl-Z` reverses it.
  `Esc` returns without restoring; F1 opens this help. Changed editor revisions,
  backing files or draft metadata refuse the operation and require fresh review.

The stores are bounded to 1 MiB each. When full, they refuse new snapshots;
old versions are preserved. Prototype schema-1 data needs explicit migration;
it is never rebound from old tree indices to current scenes.
Passage comparison additionally refuses bodies over 2 MiB, more than 16,384
diff runs or 4,096 change groups. It never shows a partial change list. Other
existing source-ownership/review bounds still apply. These byte positions are
temporary and never replace the shared durable document IDs.

## Missing document identity metadata

The shared `.grimoire/writingdeck/review-notes.json` registry connects saved
story data to its original documents. If it is lost, empty or incomplete while
milestones, variants or continuity records still refer to its IDs, WrOS refuses
to create replacement IDs or restore through them. It never guesses from the
current outline order, scene titles or matching text. Your source and sidecars
remain intact; ordinary writing, save, Undo and help remain available.
Complete any pending document-move reconnection before allocating new story IDs.

Restore the original registry from the same project's known backup before
using those saved story records. Keep the existing files when investigating an
unavailable or corrupt store. This guard does not automatically reconstruct a
lost registry, certify a backup or make cross-process metadata changes atomic.

## Read chapter aloud

Choose **Read chapter aloud** in the command palette or Writing tools to queue
the current scene and following included scenes in the same chapter. During
read-aloud, press `Space` to pause or resume, `Left`/`Right` to step by
sentence, `m` to mark the highlighted sentence in **Revision notes**, or `n` to
advance to the next queued scene.
