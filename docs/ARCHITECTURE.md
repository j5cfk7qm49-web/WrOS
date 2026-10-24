# WritingDeck OS architecture

## Scope and non-negotiable boundaries

WritingDeck is a Debian-based writing appliance, not a custom kernel. The principal UI remains Rust/Ratatui Grimoire. Ordinary novel writing must remain offline, unprivileged and usable without AI, a GPU, cloud authentication or a desktop. The project is independent and has no implied NaNoWriMo affiliation.

Preserve upstream Markdown/YAML, atomic saves, conflict checks, history, resume and recovery. Add versioned state alongside them; never migrate manuscript text merely to implement a UI widget. [AUDIT](AUDIT.md) is the baseline.

## Layers

```text
UEFI / Debian GRUB -> maintained Debian kernel + initramfs -> systemd
  -> console-setup -> WritingDeck committed keymap -> tty1 PAM login
  -> non-root writingdeck-session -> grimoire
       |
       + Ratatui + App (existing editor/autosave/history/conflicts)
       + Deck (home, primary workspaces, tutorial, cached settings)
       |   + bounded request queue -> system worker -> writingdeck-sys::Backend
       |       + Linux: fixed nmcli/brightnessctl/systemctl/pkexec arguments
       |       + Simulated: explicit label, no hardware/network writes
       + grimoire-core (ordinary files and new portable domain services)
       + grimoire-app (existing portable mobile/front-end service API)

privileged keyboard helper: fixed layouts only, root-owned files only
  -> independent 15-second systemd timer -> restore committed layout
```

`crates/writingdeck-sys` knows OS capabilities but not manuscript contents. It never receives a book path or text. `grimoire-core` and `grimoire-app` remain free of Linux privilege/service dependencies. The TUI uses both layers. External Python analyzers and cloud adapters are future workers, never synchronous input-loop work.

## Implemented vertical slice

- Debian 13 (trixie) rootfs recipe and regular-file UEFI GPT disk assembler; no custom kernel, desktop, block-device formatter or host mount operation.
- tty1 session wrapper refuses root; PAM remains the authentication boundary. VM-only autologin is conspicuously marked and networkless acceptance tests use a disposable COW layer.
- Fixed system request types and backend trait; bounded background command queue, bounded captured command output, subprocess deadline, explicit unsupported/error state.
- Root-owned console keymap application before tty1 login and an independent systemd preview/revert transaction.
- Home widgets derived from existing project/session data, hide/reorder configuration, primary-editor workspace slots/cursors and outline width, additive safe shortcut aliases.
- Strict focus hides retained references and removes them from focus traversal.
- All 39 tutorial lessons cataloged, 15 currently wired to real application events; unavailable lessons are not fake-completable. Writing exercises require a separately created demo project.

This slice is not the full requested OS. See [FEATURE_MATRIX](FEATURE_MATRIX.md).

## Persistence and concurrency

| State | Location | Ownership / behavior |
|---|---|---|
| Novel and notes | Existing Markdown + `novel.toml` | Existing core is the source of truth; YAML frontmatter remains raw |
| Autosave/history/recovery | Existing `.grimoire/` facilities | Unchanged save guards and recoverable copies |
| Selected primary workspace, pane width/cursor, widgets, launch/low-power preferences | `.grimoire/writingdeck/layout.json` | Schema v1; atomic guarded save; unreadable/future schema preserved |
| Tutorial progress | `.grimoire/writingdeck/tutorial.json` | Schema v1; verified event, skip and replay state; guarded save |
| Shortcut aliases | `paths::data_dir()/writingdeck/shortcuts.json` | Per-machine v1; conflicts/unreadable files are not overwritten |
| Committed console layout | `/etc/writingdeck/keyboard.json` | Root-owned; application changes only through helper/polkit |
| Preview token + monotonic deadline | `/run/writingdeck/keyboard-preview.json` | Root-only, boot-ephemeral; never a manuscript setting |
| Package/image identity | `/etc/writingdeck/release`, `build/appliance/packages.lock.tsv` | Recipe/profile/base and actual resolved package inventory |

`sync::save_guarded` detects ordinary external state changes. It is not a lock against an unrelated writer racing between check and rename. A future multi-process document manager must enforce one authoritative in-process buffer per canonical path and refuse unsafe concurrent writes. Unknown schema versions must be rejected rather than normalized into older state.

Workspace restoration deliberately gives the existing newer resume record precedence on startup. Explicitly choosing a workspace restores its own saved primary position. Invalid UI preferences must never prevent manuscript autosave.

## Planned domain services, in dependency order

1. **Document/pane registry:** reuse `Editor` and project nodes; one canonical document identity, independent buffers for distinct documents, source fingerprints, undo/selection/cursor, autosave/recovery, dirty/external-change arbitration. Multiple views of the same file share its authoritative document rather than competing saves.
2. **Pins/journal/scratchpad/handoff:** pin records reference original Markdown paths; cards store presentation/semantic anchor metadata only. Journal auto metrics are a separate append-only event source from manual reflections. Deletion uses existing recoverable trash semantics.
3. **Productivity:** monotonic session clock with configurable idle cutoff; edit-operation ledger distinguishes gross/net/deleted words and drafting/revision; civil local-date buckets for goals and challenges. No mandatory streaks.
4. **Challenge corpus/Story Spark:** locally versioned source-tagged records, meaningful combinatorial constraints, persisted random seed/history/favorites and date assignments. No network generator or inflated template count.
5. **Diagnostics:** revision-tagged jobs, source maps and common UTF-8 byte ranges; Harper in process, optional Nabokov/proselint workers, cancellation/stale-result discard, deduplication and explicit user-only application of suggestions. Dialogue and narration policies differ.
6. **Continuity/revision:** explicit facts with scene references, quote/context text anchors, named immutable checkpoints and reversible variants. Inferred issues remain suggestions, not definite contradictions.
7. **Publishing/backup:** one export validation model; real footnote parts/pagination tests; opt-in versioned, cryptographically verified off-device backups; a separate conflict-aware sync protocol. Never confuse local Git history, rclone copy and Google Docs round trips.

## Security and services

Use NetworkManager/logind/UPower/brightnessctl and maintained Debian services; do not create a root application server accepting arbitrary commands. The first helper accepts only four registered layout IDs and token-bound preview/confirm/cancel. The policy grants only that exact executable to active local members of `writingdeck`; there is no passwordless general sudo rule.

Power requests are user-confirmed and require successful manuscript saves. Normal development does not install appliance login/polkit policies. Timezone, sound, updates, first-run account provisioning and screen-lock integration remain planned. An upstream authenticated tty2 is the maintenance path; a passwordless VM demo cannot provide production authentication or recovery guarantees.

Production release gates: encrypted Debian installation, authenticated login and lock/suspend interaction, recovery secrets, firmware/Secure Boot policy, signed update source and tested rollback. The present GRUB fallback image is unsigned. Never deploy its passwordless profile to a physical laptop.

## Performance design

No system command runs from drawing or key handling. OS status is on demand and cached; the backend has no manuscript data. Low-power UI mode reduces redraws to one second and disables the spectrum; input events still wake immediately. Python/spaCy whole-book jobs must be opt-in/debounced and benchmarked before shipping. Their budgets are not inferred from the current Grimoire VM RSS.

## Verification

Unit tests exercise persistence, invalid versions, conflicts, safe alias validation and monotonic preview state. Rootless Python tests inject the helper's runner/clock. Real QEMU UEFI tests check the actual tty1 screen, non-root process, polkit eligibility, real console mapping and systemd-timed revert. [TESTING](TESTING.md) separates those from all remaining hardware/credential tests.
