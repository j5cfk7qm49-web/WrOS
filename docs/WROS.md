# WrOS — Writer’s OS

**Official product name:** WrOS (Writer’s OS)
**Previous working title:** WritingDeck OS (historical only)
**Repository:** https://github.com/j5cfk7qm49-web/WrOS
**Editor:** Grimoire, based on GrimoireTUI (retain upstream attribution, MIT notices and compatibility)

WrOS is a **dedicated, offline-first Linux writing appliance**: a complete boot-to-writing OS, not just a TUI application. It targets low-resource laptops with QWERTY/Colemak/Colemak-DH input, a manuscript editor and dashboard, sticky/reference panes, NaNoWriMo-oriented goals and daily challenges, Story Spark creative prompts, offline grammar/prose analysis, a guided tutorial, safe backup and operating-system management.

## Current release work

RC10 defaults to graphical disk unlock, loading, login and writing. The installed
session keeps a separate authenticated lock and plain startup recovery. The
release checklist covers the package update, startup VM and physical HP Stream
checks. [Graphical screens](GRAPHICAL_SESSION.md) · [Release checklist](RELEASE_CHECKLIST.md)

## Read next

- [Status history, to 2026-10-31](archive/STATUS.md)
- [Roadmap, to 2026-10-31](archive/ROADMAP.md) and [feature matrix](FEATURE_MATRIX.md)
- [Architecture and persistence](ARCHITECTURE.md), [technical risks](RISKS.md)
- [Linux development, image building and installation](INSTALLATION.md)
- [OS-level Colemak and keyboard support](COLEMAK.md)
- [Story Spark and daily challenges](STORY_SPARK.md), [daily goals](DAILY_CHALLENGES.md)
- [Writing tools and linting](WRITING_CHECKS.md)
- [Tutorial progress](TUTORIAL.md), [test evidence](TESTING.md)
- [Brand migration and backward compatibility](NAMING_MIGRATION.md)
- [GitHub issues](https://github.com/j5cfk7qm49-web/WrOS/issues)

The remaining old names are saved-data formats, compatibility links on installed systems and the old CLI alias, not the product brand. [Naming migration](NAMING_MIGRATION.md) lists them and why each stays.
