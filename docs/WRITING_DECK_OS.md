# WritingDeck OS

**0.1.0-dev: tested appliance foundation + M3b independent editors, not a production-ready complete OS.**

WritingDeck is an offline-first, keyboard-oriented Linux writing appliance built around GrimoireTUI. It uses Debian 13 Stable, not a custom kernel, and does not require a conventional desktop. It is independent of NaNoWriMo and other organizations.

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

No network device was present during the VM acceptance run. Existing manuscripts, dictionary spelling, ordinary editing, autosave/history/recovery and native export remain ordinary offline features. This does not mean the later requested grammar/corpus/continuity/cloud features already exist.

## Important limits

The VM-only profile uses passwordless tty1 autologin with locked passwords. It is **not safe for a physical laptop**. Its bootloader is unsigned. The production disk installer, lock-before-suspend, automatic updater/rollback, full multi-pane content presets/pins/journal, advanced metrics/challenges/Story Spark, grammar/prose/lexicon integration, continuity, genuine footnotes and Google Drive enrollment are not complete. Optional external runtime tools are not all installed in the minimal image.

The reproducible scripts and small evidence are retained. Large generated images live under ignored `build/`; regenerate them from the recipe rather than treating an ephemeral build artifact as a signed release.
