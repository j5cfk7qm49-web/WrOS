# Codex / ChatGPT Work — Resume and Complete WrOS (Writer’s OS)

**Official product:** **WrOS — Writer’s OS**
**Core writing environment:** **Grimoire**, based on the existing GrimoireTUI codebase
**GitHub:** https://github.com/j5cfk7qm49-web/WrOS
**Issue tracker:** https://github.com/j5cfk7qm49-web/WrOS/issues
**Source attachment:** `WrOS_GitReady_CodeX_Handoff.zip`

> **Branding:** “WritingDeck OS” was the former working title. The official name is now **WrOS — Writer’s OS**. Preserve `writingdeck/*` Git branch names, historical commits, existing internal paths, metadata formats, command aliases and migration compatibility until a tested change is ready. Do not globally search-and-replace identifiers. See [issue #14](https://github.com/j5cfk7qm49-web/WrOS/issues/14).

## Mission

**Latest user directive: reuse maintained FOSS first.** Audit remaining work and
existing implementations against Debian packages; use pinned upstream builds
when suitable components are not packaged. Then integrate them and continue
finishing WrOS. Read [the component decisions](FOSS_REUSE.md) before expanding
custom infrastructure. Native UI, manuscript compatibility and actual safety
qualification remain required.
The user's interface preference is **everything TUI where possible**, including
the reused installer. Keep Debian Installer in text mode; use native TUI
frontends for maintained headless services rather than adding a desktop.
The desired installation workflow is a live WrOS image with its own installer
TUI backed by Debian's existing installation components and supported frontend
interfaces. Retain standard text recovery and verify live/installer kernel matching.

Latest development checkpoint: `codex/console-lock` builds on the integrated
native installer (#40) and qualified timezone controls (#41). Manual vlock/PAM
handoff is under qualification; consult [current status](STATUS.md) and
[console-lock boundaries](CONSOLE_LOCK.md). Sleep remains disabled. Older
milestone instructions and evidence below retain their historical scope.

**Platform scope:** WrOS is the platform. Active CI qualifies the Debian 13
amd64 runtime; do not restore macOS or Windows jobs. Imported Grimoire publishing
workflows are preserved outside `.github/workflows`. See [CI](CI.md).

Continue the **actual recovered implementation**, not a new application or speculative architecture exercise. WrOS is a complete, offline-first, Debian-based **writing operating system** for low-powered laptops, not just a terminal editor. Its primary interface is Grimoire, but the deliverables include a bootable/maintainable OS image, safe installation, user authentication, OS-level Colemak and Colemak-DH, system settings, recovery, a complete fiction-writing workflow, a guided tutorial and optional safe off-device backup.

**Do not present the prototype as a production-safe OS.** Preserve original Grimoire licensing, credits, portable Markdown manuscript files, atomic saves, external-change safeguards and user work. The original Grimoire remains the editor, not the name of the entire operating system.

### What is already present (verify, do not assume)

Historical M7a code and test evidence cover a QEMU/OVMF boot-to-Grimoire prototype, console layout switching, writing dashboard/workspaces, two independently editable documents, contextual pinned notes with independent scroll restoration, writing journal/scratchpad/session handoff, measured goals/sprints/achievements/daily challenges, offline Story Spark (1,047 plot situations, 84 creative exercises, 30 daily exercises), interactive tutorial foundations and opt-in Harper grammar. The interrupted M7b tree adds substantial **unqualified** WordNet lexicon, Nabokov/proselint prose analysis, author-voice profiling and revision-dashboard code.

Recorded tests and archived VM evidence are **historical claims for the exact prior source versions**, not fresh proof that this archive builds, that M7b works, or that a physical laptop is ready. Verify every claim against code and test records.

## Step 1 — Import the recovered source into GitHub, safely

**The public GitHub WrOS repository has a short placeholder `main` history and issue tracker, but may not yet contain the recovered project source. Inspect the remote before acting.**

1. Extract the attached ZIP preserving `.git`, file permissions, symlinks where legitimate, and all source. Check for unsafe ZIP entries. Read `README.md`, `docs/CODEX_CONTINUATION.md`, `docs/HANDOFF_STATUS.md`, `docs/STATUS.md`, `docs/ROADMAP.md`, `docs/FEATURE_MATRIX.md`, `docs/RISKS.md`, `docs/NAMING_MIGRATION.md` and `docs/TESTING.md`.
2. Run `git fsck --full`, examine `git log --all --graph --oneline`, compare branch heads and inspect `git status`. **Run a full history and working tree scan for credentials, tokens, personal manuscripts, private data and inappropriately licensed assets before publishing**; WrOS is public.
3. The branch `writingdeck/foundation` must remain at historical last-qualified M7a commit `3360ec5`. The recovered `writingdeck/m7b-wip` branch contains the interrupted and **unverified** M7b checkpoint followed by a documentation-only WrOS naming handoff. The latter must not be confused with qualified M7b functionality. Inspect the actual latest SHA rather than relying on an old hash.
4. Use `bash tools/push-wros-source.sh` (or equivalent authenticated Git) to push **both branches without force**. Do not overwrite, reset or delete the GitHub `main` branch. Preserve ancestry. Verify branch heads, files, licensing and commit history directly on GitHub.
5. Reconcile the unrelated placeholder branch responsibly: select the last verified source branch as default via supported repository settings, or explicitly integrate the placeholder changes without losing history. Do not merge the unverified M7b branch merely to make it the default.
6. Close [#1 — remote source import](https://github.com/j5cfk7qm49-web/WrOS/issues/1) **only after the source is actually visible in GitHub**. Make reviewable PRs for subsequent work.

## Step 2 — Technical audit and issue triage

Do not start over from the original prompt. Audit real implementation and feature status. Examine Rust crates, TUI routing, source-preserving Markdown buffer ownership, editor history, portable metadata, Python workers, OS image build scripts, Debian packaging, keyboard recovery, suite fixtures and actual test outputs.

Review [all open issues](https://github.com/j5cfk7qm49-web/WrOS/issues), beginning with P0 priorities. For each, record current code state, correctness/security gaps, prerequisites, test plan and acceptance gates. Create or update GitHub issues if you discover reproducible defects. Do not close issues for passing mocks alone when real integration was required. Keep `docs/STATUS.md`, `docs/ROADMAP.md` and `docs/FEATURE_MATRIX.md` aligned with code.

## Step 3 — First code priority: qualify the unfinished M7b ([#2](https://github.com/j5cfk7qm49-web/WrOS/issues/2))

The WIP already adds:

- Offline WordNet senses, synonyms/antonyms and UI.
- Nabokov prose analysis and standalone proselint adapters (Vale not required).
- Fiction-sensitive sentence readability, passive voice, adverbs, wordiness and cliché findings.
- Revision dashboard, diagnostic queue and author-voice profiling.
- Versioned Python/spaCy/NLP resource provisioning, manifests, and new Rust/Python/PTY tests.

**Build and inspect before rewriting.** Validate library versions, licenses, source provenance, model/dictionary archives, exact hashes, optional offline installs, worker lifetime/cancellation, UTF-8/Markdown span mapping, dialogue/narration policy, suppressions, deduplicated diagnostics, application of suggestions only with explicit consent, undo/redo, stale-result handling and offline behavior.

Run Rust `cargo fmt --check`, normal and no-audio workspaces/tests, Clippy (warnings denied where appropriate), Python unit/integration tests, real CLI/PTy tests, simulated no-network use, and reproducible QEMU smoke tests when tools permit. Benchmark cold/warm NLP CPU/RAM and typing latency on constrained hardware/VM; distinguish emulated performance from physical device performance. Do not delete, weaken or falsely mark tests passed. Commit corrections and merge only once acceptance criteria are met.

## Step 4 — Complete the entire WrOS backlog

Prioritize data safety and OS security alongside writing features. Work incrementally by dependency, keep useful verified milestones remotely, and continue as far as the runtime permits.

### Operating-system and release foundations

- [#3](https://github.com/j5cfk7qm49-web/WrOS/issues/3): production installer with guarded disk selection, authenticated first boot, optional LUKS, firmware/UEFI checks, Wi-Fi/timezone/layout onboarding. **No actual disk writes without explicit authorization.**
- [#4](https://github.com/j5cfk7qm49-web/WrOS/issues/4): secure lock-before-suspend/input-reader handshake; never let password keystrokes reach manuscript buffers.
- [#5](https://github.com/j5cfk7qm49-web/WrOS/issues/5): signed update strategy, recovery boot and rollback; preserve manuscripts.
- [#13](https://github.com/j5cfk7qm49-web/WrOS/issues/13): CI, reproducible images, complete dependency/license notices, VM and later authorized physical-hardware qualification.
- [#14](https://github.com/j5cfk7qm49-web/WrOS/issues/14): **WrOS — Writer’s OS** naming everywhere user-facing. Migrate old filenames/CLI/config/service identifiers only through explicit backward-compatible changes and tests. Don't rewrite Git history, break existing `grimoire writingdeck ...` aliases, or erase Grimoire upstream attribution.

### Fiction and manuscript workflow

- [#6](https://github.com/j5cfk7qm49-web/WrOS/issues/6): continuity tracker for explicit source-linked facts, scene chronology, character knowledge, locations, objects and unresolved plot threads; don't hallucinate contradictions.
- [#7](https://github.com/j5cfk7qm49-web/WrOS/issues/7): robust inline anchored annotations/revision tasks, named draft milestones and reversible alternative scene versions, integrated with existing history.
- [#8](https://github.com/j5cfk7qm49-web/WrOS/issues/8): offline chapter/multi-scene read-aloud with live sentence highlighting and revision annotation.
- [#9](https://github.com/j5cfk7qm49-web/WrOS/issues/9): Markdown-linked contextual footnotes, true OOXML DOCX bottom-of-page footnotes and **verified** page-level PDF rendering; never silently substitute endnotes.
- [#10](https://github.com/j5cfk7qm49-web/WrOS/issues/10): opt-in Google Drive **versioned backup with verified restore**, distinct from conflict-safe sync and experimental iPhone/Google Docs interoperability. No user credential or manuscript upload without approval.
- [#11](https://github.com/j5cfk7qm49-web/WrOS/issues/11): complete hands-on, safe-demo interactive tutorials across existing and new OS/writing features, plus context-aware F1 help and Colemak/QWERTY lesson support.
- [#12](https://github.com/j5cfk7qm49-web/WrOS/issues/12): independent originality/editorial and licensing review of Story Spark’s 1,047 plots, 84 exercises, and 30 daily challenges. Hash uniqueness and AI-agent review are not independent legal/content clearance.

### Experience constraints that apply to every issue

WrOS must be a **coherent writing OS**, not a branded collection of scripts. Preserve local-first plain Markdown, reference-only pinned notes, independently editable panes and independent scroll/heading anchors, focus mode with no unsolicited achievement popups, scene/novel goals, NaNoWriMo-style progress, daily challenges, Story Spark's **random plot, random challenge, combined plot+challenge and daily** modes, individual rerolls, Challenge Roulette, favorites, safe source actions, offline dictionary, Harper grammar and advisory Hemingway-like Nabokov/proselint analysis. Preserve QWERTY/Colemak/Colemak-DH **at Linux console level** with recovery, low CPU/RAM use, screen/battery/network/power controls, a working interactive tutorial and safe import/export. Never require subscription or cloud AI for the core workflow.

## Working rules and completion criteria

- Use PRs, issues, tests, actual test logs and narrowly scoped commits; keep backward compatibility with Grimoire manuscripts, metadata, settings, keybindings and scripts. Write migrations when needed.
- Preserve legal notices and any copyleft/data-license obligations; separate optional distribution packs where necessary.
- Build OS/VM qualification early; do not defer all installer, authentication, recovery or release testing until the end.
- Never conflate an app running in QEMU with a production installer or secure laptop. Do not claim validated batteries, power consumption, real devices, credentials, OAuth, cloud sync or PDF layout without relevant tests.
- Close tracker issues only when their documented acceptance criteria pass. Mark test gaps and blockers clearly.
- If work must stop, push verified increments and create a precise continuation record of changed files, branches, exact test commands/results, unresolved blockers and next actions. Don't promise background completion outside the active execution.

**Start with safe import of the attached Git repository into `j5cfk7qm49-web/WrOS`; verify its visibility on GitHub, audit current code, and then finish M7b before advancing to the remaining issues. Use the official product name WrOS — Writer’s OS everywhere new user-facing content is created.**
