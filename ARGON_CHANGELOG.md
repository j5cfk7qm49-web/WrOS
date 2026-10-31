# Argon Handover Changelog (`WrOS` — `1b793b4ef37b71234ad3faa66c205e9c6bc6981c` Baseline)

## Verification & Delivery Status

- **Contribution status:** `UNVERIFIED SOURCE CONTRIBUTION` (`Implemented` / `In progress` pending full local Debian 13 Rust/Clippy/PTY/VM/hardware qualification by the receiving developer).
- **Baseline commit:** `1b793b4ef37b71234ad3faa66c205e9c6bc6981c` (`development`, matching qualified feature head `e67c89c0f156886d408d795e7d97dc2aac4d338f`, source fingerprint `8419e7df4740396394a2aaa3685eb9780330f8b78664c167ebcae635a025fe49`).
- **Environment constraints:** The remote Argon environment has no Rust toolchain (`rustc`, `cargo`, `clippy`, `rustfmt`), no `restic` or `rclone` binaries, no QEMU/KVM VM execution, and no physical hardware or live Google account credentials.
- **Dependency & lockfile status:** `Cargo.toml` and `Cargo.lock` are **unchanged** (`0` crate additions or modifications). All Rust changes reuse existing locked workspace dependencies (`zeroize`, `serde`, `serde_json`, `anyhow`, `ratatui`, `crossterm`, `libc`). No `cargo update` or lockfile regeneration is required.
- **Historical evidence:** All files under `docs/evidence/` and `HANDOVER_BASELINE.json` are preserved untouched.

---

## Implemented Behavior

### 1. Protected Native Wi-Fi Enrollment & Ordinary Connection Management
- **Protected secret type (`crates/writingdeck-sys/src/lib.rs`):**
  - Added `WifiSecret(String)` with redacted `Debug` (`"[hidden]"`), `#[serde(skip)]` on `Request::ConnectWifi { ssid, password }`, and volatile byte-by-byte zeroing on `Drop` followed by a `SeqCst` compiler fence.
  - Added `Request::ActivateSavedWifi(String)` for reconnecting saved NetworkManager profiles by exact 36-character UUID.
- **Strict input validation (`crates/writingdeck-sys/src/system_views.rs`):**
  - Added `validate_wifi_ssid(ssid)`: requires 1–32 UTF-8 bytes, no control characters, and no leading `-` (preventing option injection into `nmcli`).
  - Added `validate_wifi_password(password)`: requires 8–128 UTF-8 bytes with no control characters.
- **Maintained NetworkManager Linux adapter (`crates/writingdeck-sys/src/linux.rs`):**
  - Extended `Runner` with `run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String>`; `ProcessRunner` pipes non-empty input via `Stdio::piped()` and closes `stdin` immediately while keeping `Stdio::null()` for all other commands.
  - `Request::ConnectWifi { ssid, password }` invokes `/usr/bin/nmcli --colors no --ask --wait 8 device wifi connect <ssid>` (or without `--ask` for open networks), writes the passphrase over private `stdin`, zeroes the temporary buffer with `scrub_bytes`, redacts any echoed secret from `stderr` errors, and verifies active connection state via `nmcli --colors no -t --escape yes -f UUID,NAME,TYPE connection show --active`.
  - `Request::ActivateSavedWifi(uuid)` runs `nmcli --colors no --wait 8 connection up uuid <uuid>` and verifies active state by UUID.
  - `Request::DisconnectWifi` queries active wireless connections and brings each down by UUID (`nmcli --colors no connection down uuid <uuid>`) without hardcoding interface names.
  - `Request::ForgetWifi(uuid)` deletes saved profiles by exact 36-character UUID (`nmcli --colors no connection delete uuid <uuid>`).
- **Native Grimoire Settings UI (`src/app/shell.rs`, `src/app/writingdeck.rs`, `src/app.rs`, `src/main.rs`):**
  - Added `Route::WifiConnect` (`WifiForm` with `Zeroizing<String>` password field, masked `[hidden]` display, Cancel/Connect/Disconnect rows, and `Escape` discard) and `Route::SavedWifiProfile` (`SavedWifiAction::{Cancel, Connect, Forget}` with Cancel selected by default).
  - Routed bracketed terminal paste and `Ctrl+V` clipboard paste in `Route::WifiConnect` into the masked password field, never into primary or secondary manuscript buffers.

### 2. Google Account Credential Lifecycle & Durable Offline Backup Queue (#28, #10)
- **Credential lifecycle & revocation/offline classification (`os/scripts/wros-backup`):**
  - Persists private `google-account.json` (`0600`) alongside `rclone.conf` (`0600`) tracking `connected`, `state` (`verified`, `reauthorization-required`, `offline`), `token_expiry`, `verified_at`, and `last_error` without storing raw OAuth tokens.
  - Added `check-google`: runs a bounded `rclone lsd wrosgdrive:` check against the saved `rclone.conf` and classifies failures into `reauthorization-required` (`invalid_grant`, `Token has been expired or revoked`, `401 Unauthorized`) versus `offline` (`dial tcp`, DNS, timeout), updating `google-account.json` and returning structured status.
  - Added `disconnect-google`: overwrites `rclone.conf` with zeroes (`fsync`) before unlinking and resets `google-account.json` while preserving local manuscripts, recovery keys, and existing encrypted snapshots.
- **Durable offline backup queue & interruption-safe retry (`os/scripts/wros-backup`):**
  - Added private `queue.json` (`0600`) and commands `queue --project <dir>`, `run-queue`, and `clear-queue`.
  - `queue` validates the project directory and records a deduplicated entry in `queue.json` without requiring repository/network availability at enqueue time.
  - `run-queue` processes queued projects sequentially:
    1. Automatically runs `restic unlock` to clear stale repository locks left by interrupted runs.
    2. Inspects existing repository snapshots for the same project path: if an earlier attempt created a snapshot after `queued_at` that is not yet in `verified.json`, `run-queue` verifies that snapshot via `restore_and_compare` into a disposable directory instead of uploading a duplicate snapshot.
    3. Otherwise runs `create_backup`, which creates the snapshot and performs full restore-and-SHA-256 manifest verification before dequeuing the project.
    4. On failure, records `attempts`, `last_attempt_at`, and `last_error` in `queue.json` so the item remains queued for later retry.
- **Native UI & CLI integration (`src/backup_service.rs`, `src/app/backup.rs`, `src/app/shell.rs`, `src/writingdeck_cli.rs`):**
  - Extended `backup_service::Request` and `Status` with `CheckGoogle`, `DisconnectGoogle`, `Queue { project }`, `RunQueue`, `ClearQueue`, `QueuedBackup`, and Google credential lifecycle fields.
  - Updated `Route::GoogleAccount` with credential state/expiry display, `Verify Google Drive connection`, `Reauthorize Google Drive`, and `Disconnect Google Drive and remove local credentials` (opening `Route::GoogleDisconnectConfirm` with Cancel selected by default).
  - Updated `Route::Backups` with `Queue saved project for offline backup`, `Run queued backups and retry interrupted jobs`, `Clear pending backup queue`, and per-item queue attempt/error rows.

### 3. Signed Updates, Honest `ext4`/`btrfs` Recovery Status & Interrupted `dpkg` Repair (#5)
- **Signed offline update & recovery helper (`os/scripts/wros-update`, `os/config/50-wros-update.rules`, `os/config/org.wros.update.policy`, `os/scripts/install-files.sh`):**
  - Created `/usr/libexec/wros-update` (`#!/usr/bin/python3 -I`, mode `0755`) with Polkit action `org.wros.update` restricted to active local `writingdeck` sessions:
    - `verify-bundle <dir>`: verifies `InRelease` (clearsigned) or `Release` + `Release.gpg` (detached) against `/usr/share/keyrings/debian-archive-keyring.gpg` (or `WROS_UPDATE_KEYRING`) using `/usr/bin/gpgv --status-fd 1`, parses the signed `SHA256:` stanza to authenticate `Packages` exact byte length and SHA-256, parses `Packages` stanzas, verifies every `.deb` archive's path, size, and SHA-256, and refuses symlinks, path traversal, or unindexed `.deb` files.
    - `recovery-status`: parses `/proc/self/mountinfo` for `/` and `/home`, inspects `/var/lib/dpkg/status` and `/var/lib/dpkg/updates` for half-configured/interrupted packages, and inspects `/var/lib/wros-update/staged/manifest.json` + `/system-update`. Explicitly sets `rollback_capable = False` on `ext4` (explaining that installing Snapper on `ext4` does not provide automatic rollback and recommending encrypted Restic project backups + Debian live/rescue media; `rollback_capable = True` only on `btrfs` with separate `/home` and `/.snapshots`).
    - `stage <dir>`: refuses if `dpkg` has an interrupted transaction, verifies the signed bundle via `verify_bundle`, copies verified `.deb` archives into `/var/lib/wros-update/staged/packages` (`0700`/`0600`), re-verifies copied SHA-256 digests, writes `manifest.json`, and points `/system-update` to `/var/lib/wros-update/staged` for `systemd-offline-update`.
    - `cancel-staged`: removes `/system-update` and cleans `/var/lib/wros-update/staged`.
    - `recover-dpkg`: checks `/var/lib/dpkg/lock-frontend` and `/var/lib/dpkg/lock` with non-blocking `flock`, runs `/usr/bin/dpkg --configure -a` non-interactively, and re-verifies `/var/lib/dpkg/status` before reporting recovery.
- **System adapter & native UI (`crates/writingdeck-sys/src/lib.rs`, `linux.rs`, `system_views.rs`, `src/app/shell.rs`, `src/app/writingdeck.rs`):**
  - Added `RecoveryInfo`, `Reply::Recovery(RecoveryInfo)`, `Request::RecoveryStatus`, `Request::StageUpdateBundle { bundle_dir }`, `Request::CancelStagedUpdates`, and `Request::RecoverUpdates`.
  - Added `Route::UpdateStageForm` (`UpdateStageForm`) and extended `Panel::Updates`, `Panel::Recovery`, `Route::UpdatesView`, and `Route::RecoveryView` so writers can inspect real root filesystem rollback capability (`ext4` vs `btrfs`), stage a GPG-verified update bundle, cancel a staged update, or repair an interrupted `dpkg` state while preserving both editor contexts.

### 4. Production Installer Target Stack Cleanup & Live Writing-Session Handoff (#3)
- **Target mount/swap/mapper teardown planning (`os/tests/fixture_storage.py`):**
  - Added `target_mountpoints(mountinfo_text, target_root)`: returns active mountpoints at or below `/run/live/installer/target` ordered deepest-first (decoding octal escapes such as `\040`).
  - Added `target_swaps(swaps_text, identity, sysroot)`: returns active swap devices in `/proc/swaps` belonging exclusively to the verified target disk and fails closed if any non-target swap device is present.
  - Added `target_mapper_teardown_order(identity, sysroot)`: discovers all `dm-*` devices in `/sys/class/block` where `belongs_to_target` holds and orders them leaf-holders-first (LVM logical volumes `LVM-*` before their underlying `CRYPT-LUKS2-*` container).
  - Added `teardown_plan(identity, mountinfo_text, swaps_text, target_root, sysroot)`.
- **Owned post-write/interruption cleanup & live session handoff (`os/tests/native_installer_guest.py`):**
  - Added `cleanup_target_stack(...)` and wired it into the `worker()` `finally:` block after terminating `backend` and `frontend` so completed, declined, or interrupted runs unmount target filesystems deepest-first, `swapoff` target LVs, `dmsetup remove --retry` target LVM LVs, `cryptsetup close` target LUKS2 containers, and verify that zero target mounts, swaps, or `dm-*` nodes remain.
  - Added `installer_readers_retired()` and `handoff_to_writing_session(...)` (`native_installer_guest.py handoff-writing`) to verify that no installer process holds console input, retire target storage state, switch back to `tty1`, and start `greetd.service`.

---

## Changed, Added, Deleted, and Renamed Files

### Added Files (5)
- `ARGON_CHANGELOG.md`
- `os/config/50-wros-update.rules`
- `os/config/org.wros.update.policy`
- `os/scripts/wros-update`
- `os/tests/test_update.py`

### Modified Files (29)
- `crates/writingdeck-sys/src/lib.rs`
- `crates/writingdeck-sys/src/linux.rs`
- `crates/writingdeck-sys/src/system_views.rs`
- `docs/API_AND_NETWORK_USE.md`
- `docs/FEATURE_MATRIX.md`
- `docs/LINUX_SYSTEM_INTEGRATION.md`
- `docs/NATIVE_BACKUP.md`
- `docs/NATIVE_ENCRYPTED_INSTALL.md`
- `docs/RELEASE_READINESS.md`
- `docs/STATUS.md`
- `docs/feature-status.json`
- `os/scripts/install-files.sh`
- `os/scripts/wros-backup`
- `os/tests/fixture_storage.py`
- `os/tests/native_installer_guest.py`
- `os/tests/test_backup.py`
- `os/tests/test_backup_pty.py`
- `os/tests/test_fixture_storage.py`
- `os/tests/test_native_installer_guest.py`
- `src/app.rs`
- `src/app/backup.rs`
- `src/app/shell.rs`
- `src/app/writingdeck.rs`
- `src/backup_service.rs`
- `src/backup_ui_tests.rs`
- `src/main.rs`
- `src/shell_ui_tests.rs`
- `src/writingdeck_cli.rs`
- `tools/verify-writingdeck.py`

### Deleted or Renamed Files (0)
- None.

---

## Tests Added (and What Was Executed vs. Unrun)

### Executed Python Tests in This Environment
1. `os/tests/test_update.py`:
   - `test_verify_bundle_accepts_valid_inrelease_and_detached_release_signatures`
   - `test_tampered_deb_packages_signature_or_unindexed_deb_fails_closed`
   - `test_recovery_status_respects_real_filesystem_layout_and_never_claims_ext4_snapper_rollback`
   - `test_stage_and_cancel_offline_update_and_interrupted_dpkg_recovery`
   - **Command & Result:** `python3 -m unittest discover -s os/tests -p 'test_update.py' -v` → `Ran 4 tests in 0.153s — OK` (using real `/usr/bin/gpg` and `/usr/bin/gpgv`).
2. `os/tests/test_fixture_storage.py`:
   - Added `test_teardown_plan_orders_deepest_mounts_target_swaps_and_lvm_leaves_before_luks_container` alongside the 6 existing target storage identity tests.
   - **Command & Result:** `python3 -m unittest discover -s os/tests -p 'test_fixture_storage.py' -v` → `Ran 7 tests in 0.009s — OK`.
3. `os/tests/test_native_installer_guest.py`:
   - Added `test_cleanup_target_stack_unmounts_deepest_first_disables_swap_and_closes_lvm_before_luks` alongside the 5 existing diskless guest tests.
   - **Command & Result:** `python3 -m unittest discover -s os/tests -p 'test_native_installer_guest.py' -v` → `Ran 6 tests in 0.007s — OK`.
4. `os/tests/test_backup.py`:
   - Added `test_google_credential_lifecycle_check_revocation_offline_and_disconnect` and `test_durable_offline_queue_and_interrupted_unverified_snapshot_recovery`.
   - **Command & Result:**
     ```bash
     python3 -m unittest os/tests/test_backup.py -k "test_google or test_durable or test_status" -v
     ```
     → `Ran 4 tests in 0.015s — OK`. (Tests in `test_backup.py` that invoke `/usr/bin/restic` and `/usr/bin/rclone` require the Debian `restic`/`rclone` packages, which are not installed in this container.)
5. `os/tests/test_native_image.py`:
   - **Command & Result:** `python3 -m unittest discover -s os/tests -p 'test_native_image.py' -v` → `Ran 4 tests in 0.269s — OK`.

### Added Unrun Rust & PTY Tests (Pending Local Debian Rust Build)
1. `crates/writingdeck-sys/src/linux.rs`:
   - `wifi_enrollment_passes_secret_on_stdin_never_argv_and_verifies_active_connection`
   - `signed_update_staging_recovery_status_and_dpkg_repair_use_fixed_verified_helper`
   - Extended `unfinished_mutations_cannot_invoke_tools_or_fabricate_service_success`
2. `crates/writingdeck-sys/src/system_views.rs`:
   - Extended `restic_json_and_wifi_uuid_validators_enforce_strict_identities` with `validate_wifi_ssid`, `validate_wifi_password`, `validate_update_bundle_path`, and `recovery_status` (`ext4` rollback-claim rejection) assertions.
3. `crates/writingdeck-sys/src/lib.rs`:
   - Extended `simulator_handles_everyday_os_requests` with `WifiSecret` redaction and `Request::RecoveryStatus` checks.
4. `src/shell_ui_tests.rs`:
   - `native_wifi_enrollment_saved_profile_actions_and_errors_preserve_both_editors_at_four_sizes` (`80x24`, `100x30`, `120x40`, `160x50`)
   - `native_signed_update_staging_and_recovery_preserve_both_editors_at_four_sizes` (`80x24`, `100x30`, `120x40`, `160x50`)
5. `src/backup_ui_tests.rs`:
   - `google_disconnect_confirmation_and_offline_queue_preserve_both_editors_at_four_sizes` (`80x24`, `100x30`, `120x40`, `160x50`)
6. `os/tests/test_backup_pty.py`:
   - `test_offline_queue_retry_and_google_disconnect_preserve_editor_in_pty`

---

## Exact Static Checks Performed in This Environment

1. **Python bytecode compilation (`py_compile`):**
   - Command:
     ```bash
     python3 -m py_compile os/tests/*.py tools/*.py os/scripts/install-plan.py os/scripts/writingdeck-keyboard os/scripts/wros-display os/scripts/wros-lock os/scripts/wros-backup os/scripts/wros-update os/scripts/wros-live-session
     ```
   - Result: `exit 0` (all Python scripts, tools, and tests compile cleanly).
2. **Repository documentation, link, hash, and feature-matrix check (`tools/check-writingdeck.py`):**
   - Command:
     ```bash
     python3 tools/check-writingdeck.py
     ```
   - Result: `exit 0`:
     ```text
     PASS: 18 required docs + STATUS, local links, 89 capability rows, 40 lessons, content/dictionary hashes, notices, offline printable reference
     RELEASE GATE: 71 all-platform Rust packages lack root-level notice files; 0 used by the Linux no-audio build. No legal/hardware/production clearance implied.
     ```
3. **Rust delimiter/string balance, LF line-ending, BOM, and pristine-evidence audit:**
   - Verified zero CRLF line endings, zero UTF-8 BOMs, balanced delimiters/strings/raw-strings across all modified `.rs` files, untouched `Cargo.toml`/`Cargo.lock`, and untouched `docs/evidence/*`.
4. **Clean unified patch application check (`patch -p1 --dry-run` and full temp-tree verification):**
   - Verified that `WrOS-Argon.patch` applies cleanly to a fresh copy of the pristine baseline tree with `0` rejects and matches `WrOS-Argon-result.zip` byte-for-byte.

---

## Known Risks & Remaining Gaps

1. **Rust compilation, Clippy, and PTY execution required on developer's Debian 13 host:**
   - Run `cargo test --workspace --locked --offline`, `cargo test --workspace --no-default-features --locked --offline`, `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`, and `bash tools/test-m7b.sh` on the Debian 13 builder.
2. **Full encrypted installation + first-boot VM run (#3):**
   - Target LUKS2/LVM/swap/mount teardown (`cleanup_target_stack`) and live writing-session handoff (`handoff_to_writing_session`) are implemented and unit-tested against synthetic sysfs/mountinfo/swaps fixtures, but full QEMU/KVM encrypted install + first-boot qualification still requires execution on a host with KVM and the Debian installer build inputs.
3. **Live Google Drive OAuth, OS-wide secret vault, and physical hardware qualification (#4, #5, #10, #13, #27, #28):**
   - Live Google OAuth provider acceptance, bidirectional sync, Docs interoperability, lock-before-sleep/resume, and physical hardware testing remain open.
