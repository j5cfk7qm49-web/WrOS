# Native environment and story-model review

## Manuscript page-footnote integration (2026-10-30)

The current [Word/PDF increment](MANUSCRIPT_FOOTNOTES.md) supersedes the
historical all-rich-format footnote guard below. It reuses the maintained
parser, existing DOCX writer and LibreOffice adapter; unsupported source and
EPUB/paperback notes still fail before output. Real PDF pages and terminal
workflows passed the [independent full Debian run](evidence/manuscript-footnotes-local.json):
1060 Rust tests per configuration, 180 Python tests with zero skips, both Clippys and 21 gates. Neither a passing package assertion nor an installed renderer alone
establishes pagination, hardware or a complete publishing workflow.

## Footnote insertion audit (2026-10-29)

PR #59 now checks exclusive editor ownership, hidden/parked/trashed/unavailable
sources and the backing-file fingerprint before inserting. Unsaved original
buffer changes remain supported when the backing source is unchanged. The
maintained Markdown parser must recognize both the exact inserted reference
and the appended definition; an unclosed fence or HTML block at the end cannot
hide the definition while the action reports success.

Focused Debian verification passed 19 core tests, 7 UI tests, no-audio
warning-denying Clippy and one actual compiled-app test covering primary and
secondary insertion, typing, save, Undo/Redo and process restart at 80×24,
100×30, 120×40 and 160×50. Full Debian CI 40916740650 and independent local qualification then passed
1022 Rust tests/configuration and 171 Python tests with zero skips. PR #59
was merged as 82ede406; local evidence is retained in footnote-audit-local.json.
The rich-format export guard remains in place; this does not qualify page
footnotes, a new OS image, installation or hardware.

The 2026-10-28 feature contribution added useful model and system-view scaffolding.
It was unfinished: three exhaustive-match errors prevented a workspace build,
and none of the new story models had editor interaction, export or speech-engine
integration. The original ten files and their tracked diff were preserved before
review. This increment continues the environment-first priority in
[the roadmap](ROADMAP.md).

## Native system views

Settings now connects the read-only network scan, saved-profile, local-account,
mounted-filesystem and cached package-plan replies to the shared Grimoire menu.
The existing background worker owns service calls; rendering and input retain
the current editor context. Explicit simulators are labelled before their first
status response as well as afterwards. Refresh retires previous results before
requesting new ones; a failed refresh cannot present an old list as current.

The Linux backend uses maintained Debian tools:

- NetworkManager's [documented terse escaping](https://manpages.debian.org/trixie/network-manager/nmcli.1.en.html)
  preserves SSID/profile colons, backslashes and significant spaces. Saved Wi-Fi
  profiles carry UUIDs. New-network credentials, connection changes and deletion
  remain unavailable pending protected enrollment and explicit target review.
- Local accounts come from bounded local account data. Missing/invalid data
  yields an error; an empty supported list stays empty. No account is invented.
- util-linux [findmnt JSON](https://manpages.debian.org/trixie/util-linux/findmnt.8.en.html)
  preserves mount paths containing spaces. Invalid/null capacity data is refused
  instead of being silently converted to zero.
- APT performs only a simulation against its existing cache. Proposed removals
  are included. The result explicitly does not establish repository freshness,
  install/download updates, schedule an offline update or qualify recovery.

The original Google backend wrote a fabricated account identity and returned
synthetic Drive documents and materialization success. Those paths now report
unavailable without modifying account files or manuscripts. Unconfigured Restic,
update installation and recovery/firmware restart requests likewise cannot invoke
their tools. The reusable typed scaffolding remains, with explicit availability.
No credential-bearing field remains in the logged/serialized system request or
shell state. There is no new password store, root grant or real-provider test.
System output above the retained limit is rejected rather than parsed partially.

## Experimental writing models

These are tested foundations under development, not completed M8/M9 workflows:

- Annotation locations return original UTF-8 byte ranges. Exact quote/context
  matching refuses ambiguous occurrences, overlapping duplicates and changed
  text. Case folding cannot generate a range into a differently sized string.
- All four new story stores reuse the existing bounded, conflict-aware metadata
  store. Schema/nested fields and identities are validated. Corrupt, future,
  externally changed, oversized and unversioned prototype data is preserved.
  No automatic migration from the prototype paths is performed.
- Continuity groups keep fact categories separate. A definite contradiction
  requires the author to declare the same single-value context on both canonical
  facts. Unscoped/changing facts only suggest review; distinct explicit contexts
  and retconned facts do not produce a definite conflict. Bounded review errors
  cannot appear as a clean partial result.
- Footnote analysis reuses the existing maintained `pulldown-cmark` parser.
  Code, escaped text, link destinations and private notes cannot supply false
  references/definitions. Multiline, missing, unused and duplicate definitions
  are covered. This is inspection, not an OOXML/PDF footnote exporter.
- Preflight follows existing private-note, TK and word-count conventions, keeps
  annotation notices on empty scenes, and refuses invalid/over-limit inputs.
  It has no compile-scope/revision binding or export hook yet.
- Sentence spans reuse pinned [`unicode-segmentation` 1.13.3](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/trait.UnicodeSegmentation.html),
  already in the dependency graph, with a narrow fiction dialogue-tag adapter.
  This new model is not connected to Grimoire's existing scene read-aloud and
  highlighting implementation in `src/voice.rs`, whose Piper, Speech Dispatcher
  and eSpeak adapters remain intact. The new model adds no audio playback,
  multi-scene queue or revision-bound highlighting workflow.

Named snapshots/variants remain bounded models. They do not replace existing
manuscript history, restore drafts or implement diff/variant interaction. Stable
project-document identity, full interaction/help/tutorial work, source-revision
guards and actual publishing/speech acceptance are still required before those
capabilities can be called complete.

## Export guard for unrendered footnotes (incremental M9 work)

The existing export pipeline does **not** yet create actual OOXML, PDF or EPUB
footnotes. To prevent a superficially successful file with literal Markdown
references or wrongly placed definitions, rich-format export now rejects
selected manuscript scenes and edition-specific front/back matter containing
public `[^label]` references or `[^label]:` definitions **before creating or
modifying files**. The guard reuses the same compiled part/chapter/word scope as
export, and respects `compile: false`, private notes and Markdown code/links.

**Markdown-only export remains available** and preserves the Markdown footnote
syntax. This is a safety measure, not a footnote renderer. It does not yet
implement contextual footnote editing, real page-bottom DOCX/PDF placement,
accessible EPUB linking or complete export preflight UI. Those remain tasks in
[issue #9](https://github.com/j5cfk7qm49-web/WrOS/issues/9).

## Editable Markdown footnote quick-insert (incremental M9 work)

Writing tools and the command palette can now create a unique Markdown footnote reference at the caret, append its same-scene definition and move the editor caret to the definition for immediate writing. Existing prose, labels and markdown are preserved; the generated insertion is one undo step. Unsafe positions (code/private notes), read-only sources, selections and overly large scenes fail without changing the editor. The normal autosave/history path remains responsible for committing the source, and programmatically inserted syntax is not misclassified as independently authored tutorial prose. This is not a separate editor store or rendered page-bottom footnote layout.

## Original-source footnote navigation (incremental M9 work)

The writing-tools menu and command palette now expose a footnote navigator.
It lists public Markdown references and orphaned definitions across scenes,
shows each definition's preview, flags missing/duplicate/unused labels, and
jumps directly to original source locations. UTF-8 parser byte offsets are
converted to editor character positions; source text remains in its original
Markdown files, and private notes, code and link destinations are excluded.
The development increment guards source hashes, backing files and successful
editor ownership before selection, blocks hidden-editor mutation through
modal input, and offers an explicit definition jump using the original editor.
The browser uses wros-shell widgets and bounded inspection. There is no second
mutable footnote store, automatic rewrite or rich-format export claim.

This is deliberately **not** the final contextual bottom-of-page footnote
editor, DOCX/PDF writer or accessible EPUB implementation. Those remain #9.

## Qualification boundary

[Debian CI](evidence/native-feature-ci.json) passed 1008 Rust tests per
configuration, 170 Python tests with zero skips, both Clippys and 20 gates.
[Provenance](evidence/native-feature-ci-provenance.json) retains the verified
artifact digest, tested merge and exact source fingerprint.

The independent [local run](evidence/native-feature-local.json) passed the same
1008 Rust/configuration and 170 Python/zero-skip sets, both Clippys and 21 gates.
[Local provenance](evidence/native-feature-local-provenance.json) binds the exact
source and compiled binary to four actual availability frames:
[80×24](evidence/native-feature-availability-80x24.txt),
[100×30](evidence/native-feature-availability-100x30.txt),
[120×40](evidence/native-feature-availability-120x40.txt) and
[160×50](evidence/native-feature-availability-160x50.txt).
These are ANSI cell reconstructions with styling omitted, not VM screenshots.

An [earlier local harness failure](evidence/native-feature-runner-signal-failure.json)
is retained separately. A probe measured ignored SIGTERM/SIGINT inherited from
the private `unshare --fork` runner; the unchanged process-lifecycle test passed
with normal signal defaults. The corrected local runner omits that option and
asserts the defaults before testing. No product assertion or timeout was relaxed.
See the [diagnosis](evidence/native-feature-runner-signal-provenance.json) and
Debian's [documented fork-mode signal behavior](https://manpages.debian.org/trixie/util-linux/unshare.1.en.html).

Focused checks and fresh integrated Debian evidence are recorded separately from
the earlier [installer composition checkpoint](evidence/native-offline-provider-checkpoint.json).
That older image does not contain these feature changes. No further full
encrypted installation cycle or physical/provider operation is part of this
review. Boot, native enrollment, verified backup/restore, signed updates/recovery,
full M8/M9 workflows and release qualification remain open in [STATUS](STATUS.md).

## Anchored revision notes on development

The new `review_notes` workflow turns the earlier annotation model into a
local editor interaction. Monotonic sidecar document IDs use project-relative
locators rather than ephemeral tree indices. Exact original UTF-8 quotes and
context guard navigation; source revisions and backing fingerprints guard
comment/status/anchor changes. Resolution records the reviewed source hash.

Notes retain earlier comments, anchors and statuses. Unknown/future/corrupt,
legacy, oversized or externally changed stores are preserved. Owned filesystem
moves persist an invalidation intent before touching source, then publish the
simultaneous locator map. Interrupted moves require explicit reconnection;
this is recoverable intent, not an atomic transaction or cross-process lock.

The queue, comment form and default-Cancel anchor review share wros-shell.
Help documents source/selection/history limits and explicit external-move
reconnection. The active editor retains caret, selection and Undo; annotation
metadata never becomes manuscript prose. Lesson 32 requires a real changed
anchor save in a demo. Complete M8, a note-history restore interface, automatic
cross-document queue navigation and large-source qualification remain open.

## Development writing qualification

The independent [corrected local run](evidence/development-writing-local.json)
passed 1037 Rust tests per configuration, both warning-denying Clippys,
173 Python tests with zero skips and 21 gates. Source is `9a734ebd...26a20`
/ 450 files; [provenance](evidence/development-writing-local-provenance.json)
retains the exact binary and source binding and four actual review frames.
The first run passed product tests but failed the documentary matrix/inventory
comparison; its raw failed receipt remains separate. The inventory was corrected
and the complete verifier then passed. Manuscript/source and verifier inputs
did not change between these runs. Full M8/M9 and installer/hardware gates remain open.

## Contextual footnote reader (next development increment)

Read footnote at caret opens a shared shell dock below the retained writing.
The maintained parser identifies original reference and definition byte ranges,
including adjacent-reference boundaries and definition EOF. The reader shows
one complete bounded public definition, scrolls with the pinned Ratatui layout
calculation, chooses among repeated references and opens original source for
normal editing. The existing navigator's v action preserves its selection on Back.
No parallel editor or durable footnote store is introduced.

Definition previews now use the parser's offset-preserving masked public source.
Stripping private notes only after slicing a definition could expose a trailing
half of a private block that crossed that boundary. Source bytes remain intact.
Changed source/backing files and ambiguous definitions refuse navigation. Limits
are 2 MiB/scene, 8192 markers and a 64 KiB definition snapshot. The [independent Debian run](evidence/footnote-context-local.json) passed
1044 Rust tests per configuration, 174 Python tests with zero skips, both Clippys and 21 gates. The rich-export guard remains intact and lesson 34 cannot claim
DOCX/PDF validation. Existing speech and history engines remain unchanged.

## Export-selection source review (development increment)

The export dialog's `p` action examines the original loaded source for the
selected parts and sample, plus the front/back matter used by the selected
editions. It lists footnote errors, unused definitions and public editorial
markers with original source lines and relative paths. It separately reports the
existing rich-footnote export block even when a footnote is otherwise valid.
It is a bounded, read-only snapshot: no links, assets, continuity, revision
notes, external-file freshness or rendered pages are checked. The report is
refused in full when its bounds are exceeded. Original source and export
settings are retained; closing the review returns to the dialog. The
[independent Debian run](evidence/export-preflight-local.json) passed 1047 Rust tests per configuration, 175 Python tests with zero skips, both Clippys and 21 gates.
This does not implement printed footnotes.
