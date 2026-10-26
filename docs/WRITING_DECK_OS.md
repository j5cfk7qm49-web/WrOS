# WrOS — Writer’s OS (historical document location)

**0.1.0-dev: tested appliance foundation + M3b independent editors + M4a source-linked pins + M4b initial journal, not a production-ready complete OS.**

WrOS is an offline-first, keyboard-oriented Linux writing appliance built around GrimoireTUI. It uses Debian 13 Stable, not a custom kernel, and does not require a conventional desktop. It is independent of NaNoWriMo and other organizations.

> **Name update:** The official product name is **WrOS — Writer’s OS**. This document retains its previous filename for compatibility; new entry point: [WROS.md](WROS.md). For current qualification, read [STATUS.md](STATUS.md), not this historical overview.

## Start here

- [Current status and evidence](STATUS.md)
- [Detailed upstream audit](AUDIT.md)
- [Linux foundation comparison](LINUX_BASE_COMPARISON.md)
- [Architecture and persistence](ARCHITECTURE.md)
- [Milestones](ROADMAP.md) and [feature matrix](FEATURE_MATRIX.md)
- [Development / image building / installation](INSTALLATION.md)
- [System boundary and security](LINUX_SYSTEM_INTEGRATION.md), [risks](RISKS.md)
- [Keyboard layouts](COLEMAK.md), [shortcuts](KEYBOARD_SHORTCUTS.md), [printable reference](keyboard-reference.html)
- [Workspaces](WORKSPACES.md), [tutorial](TUTORIAL.md), [testing](TESTING.md)
- [Writing tools](WRITING_TOOLS.md), [Story Spark](STORY_SPARK.md), [challenge library](CHALLENGE_LIBRARY.md), [daily challenges](DAILY_CHALLENGES.md)
- [Backup and mobile](SYNC_AND_BACKUP.md), [content rights](CONTENT_LICENSES.md)

## What runs now

The tested VM boots its complete UEFI disk into a real non-root Grimoire session. F12 opens the home screen; F10 independently toggles the left outline column; F11 chooses/toggles an independent second editor; Ctrl-D is strict Primary-only focus. Both documents' cursor/selection/free-scroll state, pane sizes, active role, configurable existing-data widgets, shortcut aliases and tutorial progress persist. System settings uses cached/off-thread Linux status and safe requests. Console layouts have an independent preview safety timer.

Pinboard/sidebar/dashboard cards now reference existing Markdown sources without copied bodies, maintain independent scroll/semantic anchors, and explicitly enter the same owner-aware editors. Scope/category/color/order/size and recoverable card archive/Trash persist independently of manuscript text. See [PINS](PINS.md); the initial [journal](JOURNAL.md) now separates recorded saved-Project counts and original reflections. Scratchpad/handoff, richer presets and gross/active-time metrics remain incomplete.

No network device was present during the VM acceptance run. Existing manuscripts, dictionary spelling, ordinary editing, autosave/history/recovery and native export remain ordinary offline features. This does not mean the later requested grammar/corpus/continuity/cloud features already exist.

## Important limits

The VM-only profile uses passwordless tty1 autologin with locked passwords. It is **not safe for a physical laptop**. Its bootloader is unsigned. Production disk installation, lock-before-suspend, automatic updater/rollback, full journal/crash-tail/archival qualification, grammar/prose/lexicon integration, continuity, genuine footnotes and Google Drive enrollment remain incomplete. M4 original-source workflows, initial M5 recorded productivity and M6 original offline Story Spark are implemented/software-tested within their documented limits; they do not complete the full OS. Optional external runtime tools are not all installed in the minimal image.

The reproducible scripts and small evidence are retained. Large generated images live under ignored `build/`; regenerate them from the recipe rather than treating an ephemeral build artifact as a signed release.
