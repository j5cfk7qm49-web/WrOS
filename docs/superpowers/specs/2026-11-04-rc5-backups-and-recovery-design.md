# rc5: automatic backups, undoing a bad update, recovery boot

## Why

WrOS is about to ship releases quickly. Every release must be safe to try:
a book is never more than a short while away from a verified backup, a bad
update can always be undone, and a computer that will not start the writing
session can still be brought back. Today backups run only when started by
hand, a completed update cannot be undone (only repaired), and recovery
needs the install media.

## Goals

1. **Automatic backups** of the open book, verified like manual ones.
2. **Go back to the previous version** of WrOS, by hand or automatically
   when a new version cannot start the writing session.
3. **A recovery boot**: a boot-menu entry that starts a small safe mode able
   to go back a version, repair packages and restore a book, even when the
   normal session will not start.
4. **A system profile in each backup**, so a fresh install plus the recovery
   key brings back settings as well as books.

## Non-goals

- Whole-disk snapshots. The root filesystem is ext4 on LUKS; snapshots would
  need a different disk layout. That belongs with the installer work.
- Backing up Debian itself. WrOS updates change only the `wros` package, so
  the previous `wros` package is what a rollback needs.
- Secure Boot (left for the installer work).

## Anti-goals

- A backup that runs while words are unsaved, or reports success without a
  verified restore (the existing gate stays: save, snapshot, restore, compare).
- A rollback that installs anything not previously verified and installed on
  this computer.
- A recovery mode that opens the computer to whoever holds it.

## Design

### 1. Automatic backups

- Applies to a book once backups are set up for it (a repository exists);
  nothing new to switch on.
- **When:** after saved changes that are not yet in a verified backup, once the
  writer has paused for 10 minutes, and at most once an hour; also as the first
  step of every update (the existing back-up-then-stage chain). *My call.*
- **Failures** retry by themselves: offline or provider errors back off
  (15 min, 1 h, 4 h) and run again when the update check next finds the network.
  The queue that exists today becomes this retry list.
- **Quiet:** a status line in Backups and on the home screen ("Backed up
  14:02 · verified"); a notice only when backups have failed for a day.
- **Retention**, so automatic backups do not grow the repository without end
  (restic `forget --prune`, after a successful backup): every snapshot from the
  last 7 days, one a day for 90 days, one a week after that. Manual snapshots
  and named ones are never pruned. *Agreed with the writer (2026-11-04).*
  Space: the repository lives on Drive or a backup drive, not the laptop;
  restic stores only changed chunks, so a novel-sized book with hundreds of
  snapshots stays in the tens of MB. The only local cost is restic's cache,
  which is bounded and safe to clear.

### 2. System profile

Each backup also stores a small `wros-profile/` folder beside the book (inside
the same encrypted snapshot): the writer's Grimoire settings, WrOS interface
preferences, keyboard layout and timezone, and the list of books with their
backup and sync settings. **Not included:** the recovery key (it unlocks the
backup), saved Wi-Fi passwords, and the Google connection token. The writer
wants Drive connection itself streamlined first (no second computer); if that
cannot be improved, the token may join the profile later. Restore offers
"Restore settings too" when a profile is present.

### 3. Going back to the previous version

- After every successful install, the engine keeps the package it replaced (the
  previous `wros` .deb, its signed index and `InRelease`) in
  `/var/lib/wros-update/previous/`, re-verified against the pinned key when it is
  used. Only one previous version is kept.
- **Go back** (Settings › Updates and the recovery boot) stages that package as
  an explicit rollback: the engine's anti-replay check (newer release dates only)
  allows exactly the retained previous version and nothing else. It installs at
  the next restart, or now if it is app-only.
- **Automatic:** after an update installs at boot, WrOS marks the boot "on
  probation". When the writing session draws its first screen it reports
  healthy and the mark clears. If two boots in a row end without that report,
  the next boot goes back to the previous version before the login screen and
  says so after login.
- The first rollback is possible from the update after rc5: rc5's own install
  is done by rc4's engine, which keeps nothing.

### 4. Recovery boot

- A "WrOS recovery" boot-menu entry that boots the same kernel into
  `wros-recovery.target`: no greeter, no network unless asked, a console menu.
- **Protected:** the menu asks for the WrOS account password (the disk is
  already unlocked by its LUKS passphrase at boot). *Agreed with the writer.*
- **Choices:** go back to the previous version; repair packages (today's
  Repair); restore a book from a local or Drive backup into a new folder (asks
  for the recovery key); show what happened (the update journal); continue to
  the normal login; power off.
- The boot menu shows the entry after a failed boot, and stays hidden
  otherwise. Debian keeps the previous kernel, so that menu also offers it.

## What is tested where

- Python unit tests for the engine (retention of the previous package, the
  explicit rollback exception, probation and the automatic rollback), the
  backup scheduler and retention, and the profile contents.
- Rust UI tests for Settings, Backups status and the home-screen line.
- The rc-vm harness for the real thing: install rc5, apply a deliberately broken
  update in a VM, and check that it rolls back by itself; boot the recovery
  entry and use each choice.
- The laptop last, after the VM.

## Left to the builder

Exact timings and wording, the profile's file list, how probation is recorded,
and the recovery menu's layout.
