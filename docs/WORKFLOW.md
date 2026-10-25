# Scratchpad and next-session handoff — M4c

**Tested original-source workflows, not a new prose database or a production-appliance certificate.** M4a [pins](PINS.md) and M4b [sampled journal/reflections](JOURNAL.md) remain enabled. Gross/deleted/exclusive-typing and active/idle time still require M5's real edit ledger.

## Multiple original scratch notes

Open **Scratchpad** through Ctrl-K, Writing tools, its home widget or dashboard `s`. Each explicit new note is an ordinary UTF-8 original at `notes/scratchpad/000001.md` (monotonic project-local ID). Nothing is created merely by drawing the widget or opening the reader.

| Key | Explicit action |
|---|---|
| `n` / `N` | Create an empty original and request Secondary / Primary |
| Enter / `e` | Edit the selected original in Secondary / Primary |
| ↑ / ↓ | Select a note in the current lifecycle view |
| `l` | Choose an existing manuscript scene link; does not alter that scene |
| `g` | Open the linked original manuscript source |
| `p` | Promote the saved nonempty note into one independent manuscript scene |
| `v` | Open that promoted scene's original |
| `x` | Cancel only a preparation whose destination is absent |
| Tab | Active → Archive → Trash reference views |
| `a` | Archive an active reference, or restore a nonactive reference |
| Delete | Trash the reference only; never deletes its Markdown |
| PgUp / PgDn, Home / End | Reader scroll, independent of either editor |
| `r` / Esc | Explicit metadata/source reload / leave |

The reader is not a prose input field. Clipboard/bracketed/control paste cannot execute its letter commands or mutate a hidden editor. Ctrl-K remains reachable from read-only Scratchpad/Handoff/Pinboard/Journal; text fields, recovery dialogs and other modal editors retain their own input contracts. F1 supplies contextual offline help.

Explicit source editing prefers an existing owner, including a retained hidden owner where permitted. Strict focus refuses Secondary. Canonical paths and Unix device/inode identity prevent creating a second known competing editor. Missing/unreadable/non-UTF-8/conflicting/read-only sources or ambiguous retained aliases are not guessed. Both editors preserve their own selection, cursor, scroll, undo, raw YAML, guarded Project saves and recovery. Drawing uses cached authoritative nodes, not file/command I/O.

### Promotion and interrupted preparation

Promotion requires clean saved owned buffers, nonempty original note prose and an actual existing linked manuscript scene. It creates `manuscript/90-Scratchpad-000001.md` using exclusive creation, with `compile: true` and source/scene provenance frontmatter. The linked scene is **not** appended to or overwritten. The original note's bytes are unchanged; later edits to either file do not synchronize.

A durable **Prepared** receipt records the expected source and independent snapshot fingerprints before creating the destination. Successful completion marks it **Ready**. Repeating `p` does not recopy a revised original, create a second scene or recreate a missing Ready target. `v` explicitly opens the promoted original for further editing.

- If an exact Prepared target already exists, an explicit retry can finalize it, even if the separate note has since changed.
- If a target differs, all versions and the receipt remain; no overwrite/adoption guess occurs.
- If the destination does not exist but the prepared source changed, restore the known saved source before retry, or explicitly `x` to cancel that unwritten preparation, then `p` to prepare the newly saved version.
- `x` never removes an existing destination or its receipt. Reserved note references whose creation failed remain explicit unavailable references, not silently reused IDs.
- Failed metadata writes do not make a failed operation appear completed in memory. A known exact snapshot can be finalized after explicit metadata/I/O repair and reopening.

Markdown content and unrelated YAML are retained using the existing frontmatter conventions; the new scene adds compilation/provenance fields and normal frontmatter separation. **Relative resource URLs are not rebased.** Review them in the independent scene before publishing. Root/bare-name wiki links still reference existing originals; the note itself is never moved. This is not publishing preflight or a live-linked duplicate writer.

## Original next-session intention

Open **Session handoff** through Ctrl-K, Writing tools, its widget or dashboard `o`. Enter requests the ordinary original `notes/handoff.md` in Secondary; `e` requests Primary. A missing original is created empty only by this explicit editing request. Write your own next action there; no intention, advice, retrospective or measured session duration is generated.

In the reader, `s` saves through the ordinary Project/editor path and arms next open only when nonempty saved prose is known. `o` explicitly toggles startup display (arming requires that saved intention). `g` returns to the optional captured manuscript position; source availability/ownership and coordinate clamping are checked rather than guessing a different document. PgUp/PgDn/Home/End scroll this cached preview, `r` reloads, Esc leaves it.

The enabled nonempty intention appears on the next application/book open, unless upstream recovery needs attention first. A new process loads the saved original; this is not merely an in-memory popup. Corrupt/future workflow metadata cannot arm or overwrite itself, but a deliberate original-handoff editing request can still use the normal owned source path. A missing/unreadable intention is not automatically regenerated. Source text is not stored in the JSON receipt.

Handoff timestamps, an App-context token, source stamp and optional position are provenance/context, **not elapsed time or proof of an OS reboot**. The tutorial's context-open event is not process attestation; separate compiled-process tests check real quit/restart behavior.

## Metadata, preservation and boundaries

`.grimoire/writingdeck/workflow.json` is bounded schema v1: at most 1 MiB and 256 scratch references. It contains only IDs, paths, lifecycle, selection, promotion receipts and handoff provenance/settings. Unknown fields at every level, future/corrupt state, external conflicts and failing I/O are rejected/preserved. Updates clone, validate and guarded-save before publishing the new state. A reader with unavailable metadata says **unavailable**, not that originals are absent. Ordinary Markdown remains authoritative.

Creating originals uses exclusive creation. Metadata precedes note creation/promotion, so later failure may retain an unavailable reserved reference or a Prepared snapshot; this is deliberate, not a cleanup that deletes originals. Starter originals have the analogous explicit-retention rule described in [WORKSPACES](WORKSPACES.md).

These containment/fingerprint/ownership checks are **not** OS locks, cross-file atomicity, hostile-filesystem-race protection, cryptographic manuscript attestation, off-device backup, or kernel/power-cut durability certification. External rename/move of convention-path notes requires restoring or deliberately managing the reference; no arbitrary replacement is inferred. Large-note/index latency, flash writes, input CPU and real crash/power-loss qualification remain open.

## Tutorial and verification

Lessons 15 and 16 are real safe-demo exercises, not Next buttons. Scratchpad requires original prose genuinely saved, an existing scene link and successful independent promotion. Handoff requires genuine original intention save/arm followed by a real new App-context open. Ordinary novels cannot launch writing exercises. Recovery and failed saves cannot manufacture success.

M4c adds **45 Rust tests** beyond M4b: 16 workflow core, three workspace/pin-set/size-migration core, one bounded pin-preview fallback, **24 `writingdeck_workflow_*` UI**, and one system-alias compatibility case. They cover lifecycle, original custody/undo, Prepared/finalization I/O simulation and retry, missing/changed targets, unknown/future/conflicting/oversized state, aliases/roles/non-UTF-8, Waiting/recovery, cached rendering, strict focus, four-size starters/promotion/handoff, pin-set filtering and hidden-selection normalization, book isolation and safe-demo events. Controlled completion-I/O tests are not actual power-loss experiments.

Three actual unprivileged compiled-app PTY methods add: original note save/link/promotion/repeat safety and **separate-process loaded-source handoff restart** at 80×24, 100×30, 120×40 and 160×50; four-size explicit Research Secondary editing/pin-set persistence/external-future-layout preservation; future-workflow byte preservation with deliberate original handoff editing and no false arm. A read-only CLI test validates known/future/nested data without changing hashes. Existing editor/pin/journal tests remain enabled. See [TESTING](TESTING.md), [current evidence](evidence/test-results.json) and [actual workflow terminal frames](evidence/workflow-pty-screen.txt).

The fresh VM evidence qualifies the networkless boot/system/keyboard subset separately. It does not press scratchpad/preset/handoff controls or certify a physical laptop.
