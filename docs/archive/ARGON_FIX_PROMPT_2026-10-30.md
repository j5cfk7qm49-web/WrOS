# Argon repair prompt

Continue developing WrOS by repairing and integrating the useful parts of your
revised contribution. Work from the latest remote `development` branch at
https://github.com/j5cfk7qm49-web/WrOS. Inspect Git status, repository instructions
and remote changes first. Preserve other people's work. Fetch and fast-forward
when possible; resolve divergence explicitly without resets or force pushes.

Your submitted archive was based on `4039600`. The remote branch has important
subsequent repairs. PRs #63 and #64 are merged, including shared durable document
identity, guarded draft restore and genuine manuscript Word/PDF page footnotes.
The qualified implementation is included in merge
`41ba8ce066863c37c5f1785e60e70dba3a7170c1`. Fetch the latest branch, including the
review and this prompt, rather than treating that hash as the final remote head.
Port useful changes selectively; replacing current files with the old ZIP would
regress working features. Some findings below describe your submitted revision,
and are already prevented by the current implementation.

Read before editing:

- `docs/ARGON_REVISED_REVIEW_2026-10-30.md`
- `docs/ARGON_REVIEW_2026-10-30.md`
- `docs/MANUSCRIPT_FOOTNOTES.md`
- `docs/STATUS.md`, `docs/FEATURE_REVIEW.md`, `docs/FEATURE_MATRIX.md`
- `docs/ROADMAP.md`, `docs/UNIFIED_SHELL.md`, `docs/FOSS_REUSE.md`
- `docs/TESTING.md`, `docs/TUTORIAL.md`, `.github/workflows/ci.yml`

The revised ZIP hash is
`ad082307c381098c01ae3b0b841597c159bbc3595e2ec2399e4ef03b906f5cf7`;
the patch hash is
`e3dab546a5a4de6dd12f2b4e811304d3a2d7af18c5bbfa85ac74431f12237a61`.
All 42 patch outputs matched the ZIP against its old base. The original inputs
and detailed local probes are preserved privately; the following reproductions
are sufficient to recreate the important failures without those helpers.

## Preserve the platform and working implementation

WrOS is Debian 13 amd64. Do not add Windows/macOS CI. Run Grimoire and tests as
an ordinary user. Use fresh generated book/HOME fixtures and isolated disposable
targets. Prioritize everyday writing and publishing over full installer retries.
No real host disk, firmware, account, credential, manuscript, network enrollment,
suspend, release or provider operation is part of this repair task.

Use Grimoire's existing UI through wros-shell: themes, widgets, spacing,
navigation, selection, help and status. Preserve Markdown/YAML, original prose,
editor ownership, caret/selection, Undo/Redo, autosave, recovery, dated history,
authorship and licenses. Preserve corrupt/future/legacy/externally changed data
on refusal. No root Grimoire, telemetry, mandatory cloud or automatic manuscript
transmission. Do not use subagents.

Reuse maintained engines. `src/voice.rs` is the read-aloud integration; keep its
Piper/Speech Dispatcher/eSpeak adapters rather than adding another speech engine.
Reuse existing scene history and the shared document registry in review notes.
Use Restic for backup, NetworkManager/libnm for enrollment and signed Debian APT
metadata/maintained transaction mechanisms for updates. Plans and models are
not completed writer-facing workflows.

## Repair the independently reproduced defects

1. **Historical index migration can select the wrong scene.** Create scene A
   and a schema-1 milestone keyed by A's project-tree index. Insert an earlier
   scene B, reload, and invoke your migration with today's index-to-path map.
   It accepts the old index, which now denotes B, and attaches A's saved text
   to B. Current indices cannot prove historical identity. Keep migration
   refused when identity is unproven, or implement explicit reviewed
   reconnection supported by evidence. A `.v1.bak` does not justify guessing.
   Preserve originals and guard against external changes during migration.

2. **Current schema-2 draft compatibility must survive.** Your replacement
   schema 2 uses `document_snapshots` and separate per-store path registries;
   current working data uses `scene_snapshots` keyed by shared durable IDs.
   Your loader rejects that existing data. Retain the current registry and
   format, including owned rename/reorder/trash/restore behavior. Any genuinely
   necessary new schema needs explicit, guarded compatibility handling and
   preservation of unsupported data. Do not add a migration API merely to make
   an old prototype test compile.

3. **Parse before typography; preserve literal syntax.** Reproduce with:

   ```markdown
   A claim[^a--b] with `"literal -- code"` and [source](https://example.invalid/source--two).

   [^a--b]: A note.
   ```

   Your exported Markdown changes code quotes/dashes and the URL's `--` to
   typographic characters. Encoding references into sentinel tokens does not
   protect the rest of the Markdown. The current branch already fixes this in
   the manuscript path with maintained parsing and `typeset::line_after`.
   Preserve that solution when adding EPUB/paperback support. Literal labels,
   code and targets must remain exact; source files must remain byte-identical.

4. **Preserve real note paragraphs and pagination.** A definition containing
   `First note paragraph.\n\nSecond note paragraph.` produces only one Word
   paragraph in your `build_ooxml_footnotes_part`. The current manuscript
   adapter preserves multiple paragraphs and has actual long-note pagination
   checks. Reuse and extend its validation rather than restoring the flattening
   builder. Test real relationships/references, repeated labels in different
   scenes, repeated references, Unicode, private-note exclusion, long notes and
   actual rendered pages. Unsupported content must refuse before changing any
   output or companion file. Keep EPUB/paperback guards until those formats
   genuinely work and pass their own checks.

5. **Use the real maintained Wi-Fi parser.** With GLib 2.84.4, your custom
   encoder/decoder round-trip passes for the synthetic strings
   ` Cafe;Net#1\[A]` and ` pass;word#123\[x]=y`, but loading the encoded values
   through real `GKeyFile` and calling `g_key_file_get_string` fails. A plain
   leading-space name succeeds. Your `\;` and `\#` escapes are not accepted
   there. Use maintained GLib/libnm serialization and readback, and test against
   it, including significant spaces, punctuation, backslashes and UUIDs. Do not
   weaken validation by rejecting legitimate names. Keep secrets out of argv,
   logs, debug/serialized requests and manuscript metadata. Protected
   authorization, permissions, failure cleanup and profile lifecycle remain
   necessary before exposing enrollment. Perform no real enrollment here.

6. **Compile against the actual repository APIs.** The untouched submission
   fails with six core diagnostics: an unescaped `"supported"` test string,
   `.trim()` on `Option<String>` YAML values, and mutation of immutable
   `book.front`. After three build-only repairs in a disposable copy, the
   workspace reports 30 further UI/CLI diagnostics. Examples include invented
   `prose_tools::run_cli`, `Project::parent` and `peak_working_bytes` APIs;
   access to private `story_state`; `?`/`ensure!` in handlers returning `()`;
   YAML `Option<String>` treated as `String`; and mutable tutorial observation
   while the PDF receiver is still immutably borrowed. Use the current working
   APIs and guards. Do not expose internals, unwrap away missing metadata,
   remove checks, or substitute success messages to silence these errors.

## Finish useful workflows honestly

The revised Restic planning and genuine GPG/hash-chain tests are improvements.
The four submitted Python helper tests pass. That does not establish an actual
consistent backup snapshot plus independent restore, a safe update transaction,
or working NetworkManager enrollment. Complete one maintained-engine workflow
at a time, with cancellation, failures, safe persistence and genuine actions.
Keep unfinished operations unavailable and preserve the current qualified paths.

Lesson 34 on the current branch requires real note-bearing Word output followed
by its matching successful nonempty PDF in a safe demo. Preserve that contract;
viewing a dock or starting conversion cannot complete it. Lessons 3, 36 and 37
remain unavailable until their real exercises exist. Your revised catalog has
37 non-null events, so correct stale 39-event/completion claims and instructions.
Local conflict resolution alone does not establish mobile synchronization.

The next publishing opportunity is EPUB footnotes with a maintained renderer,
proper same-file note/backlinks, accessible roles, preserved paragraphs and
unique IDs across scenes/front/back matter. Test genuine EPUB packages with a
maintained validator. Existing native EPUB without footnotes passed Debian
EPUBCheck 4.2.6 with zero errors/warnings. In a Debian test environment its
explicit entry point is `java -Xmx256m -jar /usr/share/java/epubcheck.jar FILE`;
do not depend on installing host kernel binary handlers. This is a build/test
component, not evidence that an OS image or every reader is qualified.

## Verification and delivery

Read and provision the actual CI/test requirements. Use the pinned toolchain
and locked dependencies. Run format/check, both feature configurations and
warning-denying Clippy, meaningful regression tests and the integrated verifier
`bash tools/test-m7b.sh`. Do not weaken assertions/timeouts, accept skipped Python
coverage or manufacture evidence. Keep source/verifier inputs unchanged during
the full run. In namespace runners, retain normal SIGTERM/SIGINT handling; do not
repeat the inherited ignored-signal `unshare --fork` problem.

The current source baseline passed 1060 Rust tests per configuration, 180 Python
tests without skips, both Clippys and 21 local gates (20 in CI). These counts
describe that source, not your next revision. Extend the current tests, including
real compiled-app workflows at 80x24, 100x30, 120x40 and 160x50. Verify original
edit/save/Undo, independent panes, stale revisions/backing files, metadata
failure, help, normal quit and fresh-process restart. For publishing, validate
real output with maintained components rather than package substring tests or
a custom encoder paired only with its own decoder.

Update status, feature matrix/machine inventory, help/tutorial instructions and
content hashes when applicable. Report exact commands, passed/failed/skipped
counts, tested commit/source identity and remaining limitations. Deliver focused
commits against current `development`, or a matching patch based on that exact
branch if direct Git access is unavailable. Push approved source changes and
use a focused draft PR for review when available. Never force-push or represent
partial models, simulation, installed packages or VM/software tests as a
completed production OS, provider enrollment or physical-hardware qualification.
