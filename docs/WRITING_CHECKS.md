# Writing checks — M7a optional local grammar

## Scope and disclosure

The **Writing checks** menu/palette action opens a static reader for the focused original editor. Harper 2.13.0, Brill 2.13.0 and POS utilities 2.13.0 are exactly pinned. This version includes a bundled POS tagger and **neural chunker**; it is not advertised as a non-AI engine. Ordinary Spellbook spelling, editing, saving, Story Spark and the rest of the workspace remain independent. No grammar runtime model download, remote analysis, account, telemetry or manuscript transmission is used.

This is the common source-map/worker/Harper slice, **not** completed Nabokov/proselint, WordNet, author voice, ranked revision dashboard, M8–M10 or a production OS. Findings are fallible review suggestions, not a literary-quality score.

## Permission and controls

Opening the reader does not initialize an analyzer or grant consent.

- `r`, then an explicit `y` after the disclosure: run once. New permission lasts only for this process. No novel/personal file is created by once-only analysis. A previously granted personal permission for this exact runtime permits subsequent explicitly requested once-only runs without another disclosure.
- `l`, disclosure, `y`: separately guarded personal and novel writes enable live checks. A matching runtime **and canonical-project-location/runtime hash** in the personal profile, plus novel consent, are required on a new process. Copying a consented novel to a new path cannot enable live analysis, even in an already-consented profile. Moving a project requires re-enrollment. This reference is not authentication against someone able to modify the personal profile or reuse an already-authorized directory.
- `l` while live: stop this process immediately; attempt both personal-binding revocation and novel-off persistence. Partial failures say which write succeeded and preserve originals. These two writes are not a cross-file transaction.
- `c`: clear results and pause this process, retaining the saved live preference.
- `s`, `k`, confirmation: revoke saved personal model permission and all location bindings. Current work stops; **other already-running instances must reload or exit**. Novel flags/mutes remain but cannot authorize a subsequent load by themselves.
- Up/Down/PgUp/PgDn/Home/End select; Tab/Right chooses an alternative; Enter locates original source; `a` explicitly applies; Ctrl-Z in the editor undoes.
- `i` ignores this finding for this pass without persistence; `n` mutes its exact engine/version/rule in this novel; `G`, confirmation, mutes that rule across novels in this personal profile. No phrase or diagnostic prose is stored as an ignore.
- Settings: profile, English dialect, footnote bodies, notes/research and relaxed-dialogue policy. `x`/`X` independently clear novel/personal mutes with confirmation; `u` explicitly reloads. Permission pages and settings scroll at 80×24. F1 opens searchable offline help.

Novel settings: `.grimoire/writingdeck/checks.json`. Personal settings: `$HOME/.grimoire/writingdeck/checks-global.json`. Reads do not create files. Both schema-1 stores reject unknown/corrupt/future/linked/conflicting data without resetting it. Personal project permissions and per-scope rule mutes are each bounded at 256. Default model permission is absent.

## Source ownership and exact coordinates

`crates/grimoire-core/src/diagnostics.rs` supplies immutable, bounded snapshots with document, explicit source unit, source role, generation and SHA-256. **EditorBody** offsets refer to native editor text, not disk-file YAML; **MarkdownFile** has a separate preamble-aware coordinate space. Internal ranges are zero-based half-open UTF-8 bytes. Reader line/columns are one-based logical Unicode scalar positions, not terminal cells or UTF-16 units.

Checked byte/scalar/UTF-16 and line-column maps reject invalid boundaries, surrogate halves, reversed/out-of-range spans and nonliteral replacement ranges. Native Markdown and plain-prose projections retain scalar order and line endings; masking never strips characters and guesses offsets back. Code, metadata, link destinations, footnote labels, private `%% … %%` and TK spans are excluded. Footnote bodies, notes and research are opt-in. Dialogue remains prose: grammar is checked in quoted speech, with heuristic context and separate stylistic relaxation.

Every replacement checks the current source/generation/configuration, expected literal, current exclusive native document lease and backing-file fingerprint, then calls the **existing Editor** replacement path with Replace origin. It is one undoable edit, not typing credit or an automatic rewrite. Markup/entities/escapes that cannot be edited literally remain manual-review findings. Hidden/read-only/unavailable/stale sources cannot receive an edit. Existing autosave/conflict/recovery remains authoritative; fingerprints are not hostile-race CAS, OS locks or authentication.

## Worker and fiction policy

The worker is created lazily, never in draw/input. One analyzer thread, at most two current/queued visible documents, bounded two-result channel, replacement/eviction, panic containment and publication-time stale rejection are supplemented by immediate actual-source checks before displaying marks or applying edits. Live sources reconcile at 100 ms; idle debounce is 900 ms, or 2,000 ms in low-power mode. Native backend failures become explicit source-unchanged statuses; thread-start failure is not retried at every tick.

Cancellation drops/supersedes work and rejects publication; it **does not interrupt an already-running native parser**. Dropping a worker never joins on the input thread. Once-only idle references are dropped after 30 seconds even when all stale slots were removed. An in-flight call may finish asynchronously; shared dictionary and allocator memory may remain until process exit. Do not claim immediate CPU abortion or memory unloading.

Harper input is capped at **65,536 original source bytes** (common snapshot ceiling 262,144), never silently truncated. Results cap at 256 findings, eight alternatives per finding and 1,024 bytes per alternative. Oversize/no-prose/failure states are explicit, not a declaration that prose is clean. Existing spelling remains available.

Drafting is conservative grammar; Revision adds selected usage/punctuation; Deep review includes more editorial rules. These are currently **Harper profiles**. Grammar is not suppressed merely because prose is double-quoted. Quotation/em-dash hints are not certain speaker attribution; proper names/apostrophes do not become quote boundaries. Style and capitalization are suggestions, not universal rules for fiction.

## Genuine supplementary lesson

Original lesson **26 remains `SpellingOpened`**; its historic completion does not become grammar evidence. Additional lesson **40** is optional and demo-only. Its launcher inserts neither prose nor permission. Opening, consenting, inspecting, navigating or ignoring does not complete it: an actual current grammar alternative must change the original demo manuscript through the normal replacement path. Undo remains available; other historical completions are retained. The original 39 IDs are unchanged.

## Executed evidence and boundaries

- Core diagnostics source/coordinate/context/alternative/preferences/engine/queue tests include genuine pinned Harper parsing and controlled fault/concurrency analyzers; those two kinds of evidence are explicitly distinguished.
- `src/checks_ui_tests.rs`: 15 controller/terminal-buffer cases, including genuine worker/highlight/apply/undo at 80×24, 100×30, 120×40 and 160×50, policy/scope/binding/future/external/secondary/idle/accounting and genuine lesson-40 postconditions. Injected contract reports are named as such.
- `os/tests/test_checks_pty.py`: five actual-binary methods; four-size source/apply/undo/restart, scopes and live permission across processes/projects, future/external refusal, typing while a genuine worker runs, and preserved legacy spelling/new grammar lesson completion. No synthetic backend reports or user manuscript.
- [Resource record](evidence/writing-checks-resources.json): sandbox **debug** no-audio binary, 86-byte Unicode and 12,740-byte/2,340-word repetitive synthetic fixtures. Record separates cold/hot end-to-end, engine and CPU time, baseline/sampled/kernel-peak RSS. This is not physical low-power, optimized production, arbitrary-prose or battery certification.
- Full regression and VM/source alignment are recorded in [STATUS](STATUS.md) and [executed evidence](evidence/test-results.json), not inferred from targeted tests. The networkless VM probe additionally runs an isolated **non-root** real grammar/apply/undo fixture under the installed binary; dashboard startup RSS is not loaded-grammar RSS.

## Models, notices and open release gates

[Model/data manifest](model-licenses.json) records nine packaged dictionary/annotation/tagger/chunker assets, individual hashes/sizes, locked archive checksums and exact upstream VCS commits. [Retained notice sources](upstream-notice-sources.json) preserve revision-pinned verbatim notices omitted by eight selected crate archives. `tools/audit-writing-checks-assets.py` checks the fetched locked graph offline; the generated installed Rust notice bundle includes these texts. No model is fetched at runtime.

Upstream Apache-2.0 declarations and integrity checks are **not** independent training-corpus/inherited-dictionary provenance, legal clearance or complete corresponding-source certification. Those release reviews remain open and machine manifests explicitly retain `distribution_ready: false`. M6 independent editorial/rights gates and M2b/M7b–M10/physical/security/provider qualification are not discharged by this slice.
