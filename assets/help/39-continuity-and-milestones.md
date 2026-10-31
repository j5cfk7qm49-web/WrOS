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
  `4` or `c` for **Contradictions & Suggestions**. Press `5` for **Character knowledge**.
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

## Character knowledge

Knowledge is an author-entered assertion about a selected explicit fact, a
character, and **Before** or **After this scene**. WrOS does not infer what a
character learned from prose, timeline order or another character's record.

- In **Facts**, select the fact and press `g`. The active original scene and
  reviewed fact are captured. Enter a character name; use `Tab`/`Up`/`Down`
  between Character, Knowledge, Point and Notes. `Left`/`Right` cycles the
  state or scene point. `Enter` saves; `Esc` cancels.
- States are **Knows the stated fact**, **Suspects it**, **Does not know it**
  and **Believes otherwise**. Describe a different belief in Notes before
  saving that last state. This never edits the canonical fact or manuscript.
- Press `5` for the active scene's records. `a` toggles all scenes, `/`
  filters character names, and `h` includes archived records. Names match
  exactly for conflict checks; the character filter is only a view.
- `e` or `Enter` revises the selected record while its original source is
  open. `d` archives or unarchives it; earlier versions are retained. `g`
  opens the registered source through the normal editor.
- `v` reviews the complete retained versions, including the fact's scope,
  certainty, source context and notes, the scene revision and your knowledge
  notes. `Page Up`/`Page Down` and `Home`/`End` scroll; `Esc` returns.

Only explicit **Knows** versus **Does not know** assertions for the same exact
character, reviewed fact, original scene, scene point and source revision are
flagged as conflicting. Different scene points or revisions do not establish a
contradiction. **Source revised** and **Fact changed/missing** preserve what you
previously reviewed. Select the current fact and create a new reviewed record
when the canonical fact has changed; older records and versions remain intact.

Saving refuses changed source revisions/backing files, moved or missing source
identities, changed facts, and externally changed or unsupported metadata.
`knowledge.json` uses the existing guarded store and shared document registry.
It is capped at 1 MiB, 4096 records and 64 prior versions per record; refusal
preserves every old record. Character names are limited to 256 bytes and notes
to 4096 bytes. More than 512 conflict pairs or a history view exceeding 256 KiB
or 4096 source lines is refused as a whole. There is no silently partial review.
Knowledge stays outside manuscript exports and does not earn typing credit.

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
milestones, variants, continuity or knowledge records still refer to its IDs, WrOS refuses
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
