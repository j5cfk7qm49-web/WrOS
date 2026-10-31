# Argon integration and remaining release work — 2026-10-31

The ZIP-only contribution targets development `1b793b4`. Its original change
report is retained in `ARGON_CHANGELOG.md`, and its imported source is retained
in commit `3587151`. That report describes Argon's execution environment and
original implementation, rather than the final reviewed behavior.

## Integration corrections

- Verify the requested active Wi-Fi SSID, rather than accepting any active
  profile. Validate personal WPA passwords as 8–63 bytes or exactly 64 hex digits.
  Refuse unsupported enterprise/WEP enrollment before opening a misleading
  password-only form. Keep password input private and scrub the form on submit.
- Apply the manuscript save/conflict gate to running the backup queue. Persist
  an operation ID before uploading, tag snapshots with that ID, and require both
  that identity and an actual restored inventory match before retry completion.
  Retain failed items and count failures that occur before an upload starts.
  Queue execution is explicit and backs up the latest saved files; queueing
  records intent and does not freeze another copy of the manuscript.
- Keep update functionality read-only: cached APT plan, signed bundle content
  inspection (`Date:` / `Valid-Until:` freshness, `dpkg --compare-versions`
  anti-downgrade and dependency/conflict checks, POSIX `F_GETLK` lock inspection)
  and filesystem/dpkg diagnostics. The submitted staging code created
  `/system-update` without an installer service. Its package lock check used
  `flock` rather than dpkg's POSIX locks, privileged paths/keyring could be chosen
  through environment overrides, signature verification reopened mutable input,
  and the eight-second parent deadline could interrupt package repair. Remove
  the privilege grants and mutation entry points. Authenticate the exact private
  metadata bytes parsed and never infer bootable rollback from btrfs directories.
- Production installer supervision (`os/scripts/wros-install-session` and
  `src/live_installer.rs`) is now implemented alongside the marked disposable-QEMU
  qualification tooling (`os/tests/fixture_storage.py`,
  `os/tests/native_installer_guest.py`). Full QEMU encrypted installation and
  first-boot execution remain required to qualify the workflow.

## Gap list and return order

These are unfinished release tasks. Closing this integration does not close them.

1. **Production encrypted installer and first boot (#3):**
   - **Implemented in source (`os/scripts/wros-install-session`, `src/live_installer.rs`, `src/app/shell.rs`, `src/app/writingdeck.rs`, `os/scripts/stage-live-session.sh`, `tools/audit-live-session.py`):**
     supervised live-session handoff from `tty1` (`wros_input::Input::pause()`) to `wros-installer` on `tty7` over the maintained Debian `cdebconf` passthrough backend, Cancel-default confirmation (`Route::InstallerConfirm`), manuscript save/conflict gate (`App::request_live_installer`), in-target encrypted proof verification (`94wros-target-proof` / `97wros-target-finished`), and leaf-first target LUKS2/LVM/swap/mount teardown (`cleanup_target_stack` in both `worker()` `finally:` and host-namespace `ExecStopPost`).
   - **Remaining qualification gap:** compile `grimoire` and `wros-installer` on the Debian 13 builder, rebuild the live-session image (`os/scripts/build-installer.sh --live`), and execute a complete encrypted installation + first boot on a freshly generated QEMU disk (`os/tests/native_installer_vm.py`), including refusal, cancellation and failure recovery. Never nominate a host disk for these tests.
2. **Signed update transaction and recovery (#5):**
   - **Implemented foundations and offline boot service (`os/scripts/wros-update`, `os/systemd/wros-offline-update.service`, `os/scripts/install-files.sh`, `os/tests/test_update.py`):**
     sealed-copy `gpgv` signature verification (`InRelease` / `Release.gpg`), `Date:` / `Valid-Until:` freshness checks, `dpkg --compare-versions` anti-downgrade and `Depends`/`Pre-Depends`/`Conflicts`/`Breaks` evaluation against `dpkg/status`, POSIX `F_GETLK` lock inspection (`posix_dpkg_lock_active`), ordered exclusive `F_SETLK` lock acquisition (`acquire_posix_dpkg_locks` on `lock-frontend` then `lock`), durable transaction journal (`prepare_offline_transaction` and `execute_offline_boot_transaction` in `/var/lib/wros-update/journal.json`), early `/system-update` symlink removal before `apt-get --no-download` execution, and the systemd offline boot unit (`os/systemd/wros-offline-update.service` linked in `system-update.target.wants`).
   - **Remaining implementation & qualification gap (`crates/writingdeck-sys/src/linux.rs`, `os/scripts/wros-update`):**
     interactive root actions (`Request::StageUpdates`, `Request::CancelStagedUpdates`, `Request::RecoverUpdates`) remain refused in `crates/writingdeck-sys/src/linux.rs` and `os/scripts/wros-update` until disposable-VM offline-boot interruption, ext4/btrfs rescue recovery, and polkit privilege gates are independently qualified.
3. **Wi-Fi service/device acceptance (#4):** qualify real NetworkManager password
   input, invalid credentials, missing services, radio loss, reconnect/disconnect
   and cancellation against a test access point (`crates/writingdeck-sys/src/linux.rs`,
   `src/app/shell.rs`). Existing adapter/UI tests do not emulate a wireless driver.
   Enterprise enrollment remains a distinct flow.
4. **Google provider and credential protection (#28, #10):** explicit live-account
   authorization/refresh/revocation, offline retry and cross-device restored hashes
   remain required (`os/scripts/wros-backup`, `src/backup_service.rs`, `src/app/backup.rs`).
   Private files and best-effort overwrite are not an encrypted credential vault
   or guaranteed secure deletion. Obtain account/data consent before live testing.
   Keep two-way sync, Drive browsing and Docs round trips separate from encrypted
   repository backup.
5. **Power and release device acceptance (#4, #13, #27):** preserve disabled
   suspend (`crates/writingdeck-sys/src/lib.rs`, `os/config/60-wros-sleep-guard.conf`)
   until lock-before-sleep/resume qualifies. Target hardware, power-cut recovery,
   current media/source inventories and release signing remain open.

## Qualification

The [complete local software receipt](evidence/argon-os-local.json) passed
1099 Rust tests per overlapping default/no-audio configuration, 227 Python
tests with zero skips, both strict Clippys and all 21 software gates. The
[provenance and fresh four-size terminal frames](evidence/argon-os-local-provenance.json)
bind the received ZIP/patch, final source bytes and tested executable. Original
named evidence is preserved. Fixtures use disposable books, private temporary
state and the non-root Debian builder; no real Google grant or target disk was used.

The secret scan of the imported commit flags one deterministic 32-hex fixture
nonce in `os/tests/test_native_installer_guest.py`. It identifies a freshly
generated test disk; it is not an account credential or recovery key. No real
account credentials were supplied with the contribution.

Maintained contracts for the next update implementation: [systemd offline update
service requirements](https://www.freedesktop.org/software/systemd/man/248/systemd.offline-updates.html),
[dpkg locking policy](https://wiki.debian.org/Teams/Dpkg/FAQ), and
[nmcli command/reference contract](https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/nmcli.html).
