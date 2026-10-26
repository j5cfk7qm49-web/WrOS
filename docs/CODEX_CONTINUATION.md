# WrOS / WritingDeck OS — Codex continuation handoff

**GitHub:** https://github.com/j5cfk7qm49-web/WrOS

**Origin:** recovered incomplete Arena.ai development workspace, preserved here with full Git history. **This archive is not the GitHub remote and is not a production OS release.**

## Current verified boundary

- The last milestone with recorded full Rust/PTY and networkless QEMU test evidence is M7a, commit `3360ec5` on branch `writingdeck/foundation`. Verify historical evidence against the exact source; never treat it as fresh verification.
- M7b additions were found in the recovered working tree, not fully qualified: Nabokov + standalone proselint adapters, WordNet lexicon, author-voice, revision dashboard, Python runtime provisioning, new tests, data/license manifests.
- This handoff checkpoints the recovered WIP without modifying the verified M7a branch. Read `docs/STATUS.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/RISKS.md` and `docs/WRITING_CHECKS.md` before edits.
- Previous independent local check: Python syntax and 23 targeted story-corpus/installer/keyboard Python tests passed. Current sandbox lacks Cargo/Rust and the exact pinned M7b Python runtime; **fresh M7b Rust/PTY/VM qualification has not been done**.
- No physical-laptop install, hardware battery, production authentication, signed upgrade, verified Google Drive backup, live OAuth or complete independent content rights clearance has been demonstrated.

## Step zero — publish code safely

1. Before publishing, scan the entire Git history and the WIP changes for keys, credentials, private novel manuscripts, copyrighted assets and oversized files. This repository is **public**.
2. Verify `git fsck --full` and inspect `git log --oneline --all --decorate`, `git status`, and branch ancestry. Preserving dangling pre-recovery objects is optional, but do not alter referenced Git history.
3. The source has two branches: **verified milestone source** `writingdeck/foundation` at `3360ec5` and **unverified snapshot** `writingdeck/m7b-wip`.
4. Push both branches to `https://github.com/j5cfk7qm49-web/WrOS.git` with `bash tools/push-writingdeck-to-wros.sh` after obtaining authorized credentials/network. **Never force-push** or overwrite the preexisting placeholder `main`.
5. Verify GitHub now has expected source files and history, then make the appropriate verified branch the default through repository settings, or integrate unrelated placeholder history explicitly.
6. Close [GitHub Issue #1](https://github.com/j5cfk7qm49-web/WrOS/issues/1) only after verifying remote import. The GitHub remote currently has a separate small placeholder history.

## Next engineering steps

1. Checkout `writingdeck/m7b-wip` as a development branch. Read [Issue #2](https://github.com/j5cfk7qm49-web/WrOS/issues/2). Audit/execute full Rust tests, warning-denying Clippy, both audio/no-audio builds, Python, linter/model lock integrity, WordNet, active PTY and networkless VM. Repair actual failures and checkpoint qualified changes on a new reviewable branch or PR. Verify precise Unicode offsets, quotations/dialogue behavior, stale results, consent, resource ceilings and no automatic rewriting.
2. After qualification, use small PRs to merge M7b into the verified default. Do not weaken tests or change manifest checks to make broken builds pass.
3. Security/release P0: [#3 installer](https://github.com/j5cfk7qm49-web/WrOS/issues/3), [#4 input-safe lock](https://github.com/j5cfk7qm49-web/WrOS/issues/4), [#5 updates/recovery](https://github.com/j5cfk7qm49-web/WrOS/issues/5), followed by [#13 CI and qualification](https://github.com/j5cfk7qm49-web/WrOS/issues/13).
4. Long-form milestones M8: [#6 continuity](https://github.com/j5cfk7qm49-web/WrOS/issues/6), [#7 annotations/variants](https://github.com/j5cfk7qm49-web/WrOS/issues/7), [#8 read-through](https://github.com/j5cfk7qm49-web/WrOS/issues/8).
5. Publishing/cloud M9: [#9 real page footnotes](https://github.com/j5cfk7qm49-web/WrOS/issues/9), [#10 Drive backup and conflict-safe sync](https://github.com/j5cfk7qm49-web/WrOS/issues/10).
6. Finish [#11 full tutorial](https://github.com/j5cfk7qm49-web/WrOS/issues/11), [#12 editorial/licensing review](https://github.com/j5cfk7qm49-web/WrOS/issues/12), and release/hardware qualification.

## Product requirements that must not be lost

- WritingDeck is a complete **offline-first, bootable Linux writing appliance** (NaNoWriMo-oriented), not just a terminal app. Main interface remains Grimoire; OS installs, system settings, recovery, secure boot path, keyboards and backup are product requirements.
- QWERTY, Colemak and Colemak-DH must be supported at the OS console level, with tested rollback; no insecure passwordless production boot.
- Preserve single-source Markdown, in-editor secondary notes, scroll/heading anchoring, source-safe history, existing Grimoire behavior, and the 1047 plot/84 exercise Story Spark corpus. Four modes: plot, creative challenge, combined, daily; independently reroll, pin, sprint and save. All content must pass independent licensing/editorial review before public distribution.
- Fully offline writing: daily challenges, sprints, goals, achievements, dictionary, Harper, Nabokov, standalone proselint, revision queue, true focus mode and guided tutorial.
- Implement continuity, anchored annotations, alternate drafts, genuine bottom-page DOCX/PDF footnotes, import/export validation, optional verified remote backup and safe mobile workflow.
- Keep all features optional where distracting, and never confuse simulated progress, self-reported creative completion, historical test records or VM operation with independently verified production behavior.

## Working agreement

Work incrementally. Keep source-data migrations backward compatible, tests deterministic, tutorial exercises honest, manuscript writes atomic, network operations opt-in, and privileged operations narrow. Track each issue with a concrete test record, reviewable PR and clear actual/blocked status. Do not stop with a high-level design; implement code and test what the environment permits. If work is too large for one session, end with an explicit resumable checklist and commits, rather than promising asynchronous completion.
