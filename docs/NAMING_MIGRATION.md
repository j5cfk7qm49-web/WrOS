# WrOS (Writer’s OS) naming and compatibility plan

**Official name:** **WrOS — Writer’s OS**. “WritingDeck OS” is a former working title; retain it only when identifying historical documentation, commit subjects or pre-migration artifacts. The primary editor remains **Grimoire** and keeps original source attribution.

## Completed in this handoff

- The standalone Codex prompt, packaged `CODEX_PROMPT.md` and `docs/CODEX_CONTINUATION.md` use WrOS as the product identity.
- The package README, current status/roadmap/architecture headings and the canonical `docs/WROS.md` identify WrOS correctly.
- The existing `docs/WRITING_DECK_OS.md` remains accessible because prior references may depend on its path.
- Safe source-import tooling is exposed as `tools/push-wros-source.sh`, with an old-path compatibility wrapper.
- The last historically qualified `writingdeck/foundation` branch and all pre-existing Git commit contents remain unchanged. The original handoff was docs-only; subsequent M7b software qualification is recorded separately in [STATUS](STATUS.md) and does not complete the remaining user-facing brand migration.

## Still requires implementation — GitHub #14

- Audit actual Rust/CLI/help/TUI/tutorial/OS image user-visible strings such as “WritingDeck dashboard”, “WritingDeck OS simulator” and CLI version display. Update user-facing branding to WrOS in a separate tested PR.
- **Do not mechanically rename** `src/app/writingdeck.rs`, crates such as `writingdeck-sys`, `os/writingdeck`, Debian service names, existing `grimoire writingdeck ...` commands, directory keys, marker strings, persisted schemas, topic/help IDs, branch names, Git history, tests or dataset source IDs without investigating compatibility.
- Add backward-compatible CLI aliases, migrations and defaults if internal identifiers do change. Test existing user projects and old settings.
- Update OS splash, installer, docs, site, screenshots, help, README assets, notices, environment variables and distribution metadata as appropriate. Retain upstream Grimoire notices.
- Run normal and no-audio Rust tests, Python/PTY/VM smoke and CLI help regression checks before claiming the brand migration is complete.

See [Issue #14](https://github.com/j5cfk7qm49-web/WrOS/issues/14).
