# WrOS (Writer’s OS) naming and compatibility plan

**Official name:** **WrOS — Writer’s OS**. “WritingDeck OS” is a former working title; retain it only when identifying historical documentation, commit subjects or pre-migration artifacts. The primary editor remains **Grimoire** and keeps original source attribution.

## Completed in this handoff

- The standalone Codex prompt, packaged `CODEX_PROMPT.md` and `docs/CODEX_CONTINUATION.md` use WrOS as the product identity.
- The package README, current status/roadmap/architecture headings and the canonical `docs/WROS.md` identify WrOS correctly.
- The existing `docs/WRITING_DECK_OS.md` remains accessible because prior references may depend on its path.
- Safe source-import tooling is exposed as `tools/push-wros-source.sh`, with an old-path compatibility wrapper.
- The last historically qualified `writingdeck/foundation` branch and all pre-existing Git commit contents remain unchanged. The original handoff was docs-only; subsequent M7b software qualification is recorded separately in [STATUS](archive/STATUS.md) and does not complete the remaining user-facing brand migration.

## rc7 installed names

- `/etc/wros` holds the committed keyboard layout, release record and image
  markers. The update carries over the old layout and markers, then links the
  old paths. The old release path keeps the field name pre-rc7 recovery reads.
- `/usr/share/wros/keymaps` and the `wros-keyboard` units are canonical. The
  previous program, unit and keymap names resolve to them. The pre-rc7 greeter
  also keeps its keyboard authorization action and session command.
- The local writer group becomes `wros` with its original numeric ID. A
  separate compatibility group with the old name gives pre-rc7 polkit and PAM
  a distinct name to resolve after a go-back. The package synchronizes its
  members at configuration, and the installer puts new writers in both groups.
- New tutorial books have a `.wros-demo` marker and carry the old marker for
  older editors. Existing demo markers still work. The update leaves the
  `.grimoire/writingdeck` book format and installed shortcut store as persisted
  compatibility paths. An older editor can still read those files after a
  go-back. Help IDs and corpus IDs keep their existing identifiers.
- `WROS_OS` is the session marker. External wrappers can still set the old
  environment variable, and `grimoire writingdeck ...` remains a CLI alias.

See [Issue #14](https://github.com/j5cfk7qm49-web/WrOS/issues/14).

## rc9 developer names

The tools (`tools/check-wros.py`, `verify-wros.py`, `wros_source.py` and the
others), the bundled content folder `crates/grimoire-core/assets/wros/`, test
names, VM test markers (`WROS_VM_*`) and the help page (`28-wros.md`, help ID
`wros`) now say WrOS. Settings read from the environment use `WROS_` names
first and still accept the `WRITINGDECK_` ones.

`os/tests/test_name_leftovers.py` fails if `writingdeck` appears anywhere else.
These names stay, each because renaming it would break something:

| Name | Why it stays |
| --- | --- |
| `.grimoire/writingdeck/` in each book | Every book's saved state; older versions read it after a go-back, and backups restore it |
| `.writingdeck-demo` | Older editors recognise the tutorial book by it |
| `writingdeck-native`, `writingdeck-original-*` | Engine and content IDs saved in review findings, voice profiles and Story Spark history |
| `paths::data_dir()/writingdeck/shortcuts.json` | Each computer's saved shortcuts |
| `/opt/writingdeck/python` and `.writingdeck-runtime.json` | The installed prose-tools runtime, pinned by hash |
| `writingdeck-source-repack-1`, `writingdeck-pinned-runtime-1` | Written into the pinned wheels, so changing them changes their hashes |
| `/etc/writingdeck`, the `writingdeck-*` units, `org.writingdeck.keyboard`, the `writingdeck` group | rc7 compatibility links, so a go-back to rc6 still works |
| `grimoire writingdeck …`, `WRITINGDECK_*` | Older command and environment names, still accepted |
| WritingDeck OS, `writingdeck/foundation` and other old branches | History |

Bundled content and the history in `docs/archive/` and `docs/evidence/` keep
their words as written.

## CLI & Helper Naming Increment (2026-10-30)

- Added `grimoire wros ...` as the primary user-facing CLI subcommand in `src/main.rs` and `src/writingdeck_cli.rs` while preserving `grimoire writingdeck ...` as a 100% backward-compatible alias.
- Added bounded local `docs-audit` comparison to both aliases. It performs no provider operation.
- Submitted backup/update/Wi-Fi helper prototypes remain outside the product pending maintained-engine integration and qualification; see [the contribution review](archive/ARGON_REVIEW_2026-10-30.md).
