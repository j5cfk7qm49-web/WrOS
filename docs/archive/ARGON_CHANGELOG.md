# WrOS Argon Source Contribution Changelog (`4eeca44` Baseline Continuation)

- **Repository:** `https://github.com/j5cfk7qm49-web/WrOS`
- **Baseline branch / commit:** `development`, `4eeca4403580c2ef239f8da262dcb201f48fe164`
- **Qualified feature head:** `b19f2f4f05ada7a689cfc48ca1a00ecd5e1dca64`
- **Baseline source fingerprint:** `f83fbdf9e9373b261fa4daba139a2bf5281a88bcdf942d0751b249ce68756eb3` (`HANDOVER_BASELINE.json`, preserved untouched)
- **Verification status:** **UNVERIFIED SOURCE CONTRIBUTION** for Rust compilation, Clippy, Rust unit/UI tests, PTY harnesses, QEMU/VM execution, live provider access, and physical target hardware. Python unit tests and repository static checks executed in this container are reported below with exact commands and results.

## 1. Summary of Implemented Workflows

### 1.1 Production Encrypted-Installer Session Workflow (#3, #24)

Implemented the production live-session installer supervisor, Polkit authorization, live-session image staging and audit gates, and native Grimoire console handoff (`tty1` -> `tty7` -> `tty1` or reboot/poweroff):

- **Production Live-Session Supervisor (`os/scripts/wros-install-session`):**
  - Verifies the live boot marker (`/run/live/medium`) and active console `tty1` before any privileged action; refuses installed systems (`INSTALLED_SYSTEM`) with zero system changes.
  - Creates a root-owned private runtime workspace (`/run/wros-installer`, mode `0700`) with a non-blocking exclusive `flock` single-instance lock (`/run/wros-installer/session.lock`) so concurrent invocations fail closed (`BUSY`).
  - Spawns a transient systemd unit (`wros-installer-session.service`) via `systemd-run --collect --wait --quiet` on dedicated VT `tty7`, running `openvt -f -c 7 -s -w -- /usr/libexec/wros-install-session --worker` and registering a host-namespace `--property=ExecStopPost=/usr/libexec/wros-install-session --cleanup-only` fallback hook so external `systemctl stop` or process termination still retires `/target` mounts, swap, LVM logical/volume groups, and LUKS2 mappings and returns the active console to `tty1`.
  - Executes the maintained Debian installer backend paired with `/usr/bin/wros-installer` over `DEBIAN_FRONTEND=passthrough` FIFO pipes (`cdebconf` passthrough protocol) inside an isolated mount namespace (`unshare --mount --propagation slave`).
  - Verifies the in-target encrypted layout proof (`/run/wros-installer/target-proof.json` written by `os/installer/finish-install.d/94wros-target-proof` and `97wros-target-finished`) before accepting completion:
    - Confirming `/target/etc/crypttab` has a non-empty `luks` entry (`UUID=...` or `/dev/...`).
    - Confirming `/target/etc/fstab` mounts `/` and `/boot/efi`.
    - Confirming `/target/etc/passwd` and `/target/etc/shadow` have a locked `root` password (`!` or `*`), a locked `_greetd` service account with `nologin`/`false`, and at least one normal writer account (`UID >= 1000`) with a hashed password (`$...`).
    - Confirming `/target/etc/greetd/config.toml` runs `/usr/bin/wros-greeter` as `_greetd` on `tty1` with no `initial_session` or `--live-session` flag.
  - Retires the target storage stack leaf-first (`cleanup_target_stack` in both the worker `finally:` block and `--cleanup-only`): unmounts nested `/target` mounts deepest-path-first, runs `swapoff` on `/target` swap devices or `/dev/mapper/wros*` / `/dev/wros_vg/*`, deactivates active LVM logical volumes (`lvchange -an`) and volume groups (`vgchange -an`), closes active LUKS mappings (`cryptsetup close`), runs `udevadm settle`, and verifies `/proc/mounts`, `/proc/swaps`, and `/dev/mapper` have zero leftover target entries.
  - Restores active VT `tty1` (`chvt 1`) when returning to the live writing session (`RETURN_TO_WRITING`, `CANCELLED`, `FAILED`, or `CLEANUP_FAILED`), or triggers `systemctl reboot` / `systemctl poweroff` only after clean target teardown when the user selects post-install reboot or poweroff.

- **Live-Session Polkit & Image Staging (`os/config/50-wros-installer.rules`, `os/config/org.wros.installer.policy`, `os/scripts/stage-live-session.sh`, `tools/audit-live-session.py`):**
  - Grants `org.wros.installer` (`pkexec /usr/libexec/wros-install-session`) exclusively to the live session account (`user` UID 1000) when `/run/live/medium` exists and the caller is on local `tty1`.
  - Stages `/usr/bin/wros-installer`, `/usr/libexec/wros-install-session`, `/etc/polkit-1/rules.d/50-wros-installer.rules`, `/usr/share/polkit-1/actions/org.wros.installer.policy`, and masks `getty@tty7.service` and `autovt@tty7.service` in `os/scripts/stage-live-session.sh`.
  - Audits all staged installer binaries, Polkit rules/actions, `tty7` masks, and live-user guards in `tools/audit-live-session.py` and `os/tests/test_live_session.py`.

- **Native Grimoire Live-Installer Handoff & 4-Size UI Coverage (`src/live_installer.rs`, `src/main.rs`, `src/app/writingdeck.rs`, `src/app/shell.rs`, `src/shell_ui_tests.rs`):**
  - Added `src/live_installer.rs` mirrors of `src/manual_lock.rs` (`LiveInstallerOutcome`, `LiveInstallerHelperError`, `run_live_installer_with`, `verify_live_installer_command_path`, `validate_executable_metadata`, and 8 unit tests covering outcome parsing, symlink/group-writable executable rejection, non-root owner refusal, and `tty1` handoff argument ordering).
  - Added `Route::InstallerConfirm` in `src/app/shell.rs` (defaulting selection to `Cancel` (`selected == 0`), matching `PowerConfirm`) and `App::request_live_installer()` / `App::take_pending_live_installer()` in `src/app/writingdeck.rs`, enforcing the manuscript save/conflict gate (`persist_dirty_buffers_for_power_action()`) before releasing the terminal so unsaved edits in either primary or secondary buffer are saved—or block handoff on an external disk conflict—before `tty1` is released.
  - Wired `main.rs` to pause the `tty1` Crossterm reader (`input.pause()`), invoke `live_installer::run_live_installer()`, resume a fresh reader (`input.resume(proof)`), re-enter raw alternate screen, clear the terminal, and record the outcome in `app.finish_live_installer(...)`.
  - Added 4-size compiled-app UI test `live_installer_handoff_confirmation_and_dirty_buffer_gate_preserve_both_editors_at_four_sizes` in `src/shell_ui_tests.rs`.

### 1.2 Signed Update Transactions, POSIX `dpkg` Lock Coordination, Offline Boot Service & Durable Journal (#5)

Extended `os/scripts/wros-update`, added `os/systemd/wros-offline-update.service`, and kept interactive root mutations refused (`crates/writingdeck-sys/src/linux.rs` and `os/scripts/wros-update` CLI) until disposable-VM interruption and rescue recovery qualification completes:

- **Repository Metadata Freshness (`parse_release_sha256` in `os/scripts/wros-update`):**
  - Parses and validates `Date:` (refusing timestamps more than 5 minutes in the future unless `allow_expired=True`) and `Valid-Until:` (refusing expired metadata when `Valid-Until` is present or `require_valid_until=True`, and refusing bundles where `Valid-Until <= Date`).
- **Maintained `dpkg --compare-versions` Anti-Downgrade & Dependency/Conflict Policy (`evaluate_bundle_against_installed` in `os/scripts/wros-update`):**
  - Parses `Pre-Depends`, `Depends`, `Provides`, `Breaks`, and `Conflicts` fields from the authenticated `Packages` index alongside `var/lib/dpkg/status`.
  - Uses `/usr/bin/dpkg --compare-versions` to refuse package downgrades, refuse modifications to held packages (`want == 'hold'`), verify versioned/unversioned `Depends` and `Pre-Depends` satisfaction across the candidate bundle + installed state, and refuse bundles that trigger `Breaks` or `Conflicts` against installed or candidate packages.
- **POSIX `fcntl` Lock Inspection & Exclusive Acquisition (`posix_dpkg_lock_active` and `acquire_posix_dpkg_locks` in `os/scripts/wros-update`):**
  - Queries `fcntl.F_GETLK` with a `flock` `F_WRLCK` probe on `/var/lib/dpkg/lock-frontend` and `/var/lib/dpkg/lock` (matching `apt`/`dpkg` POSIX record locks rather than BSD `flock()`), reporting active lock state in `inspect_recovery` without creating or mutating lock files.
  - Acquires ordered exclusive non-blocking POSIX write locks (`fcntl.F_SETLK` with `F_WRLCK` on `lock-frontend` first, then `lock`) via context manager `acquire_posix_dpkg_locks` for offline boot transactions.
- **Durable Offline Transaction Journal & Early Trigger Removal (`prepare_offline_transaction`, `execute_offline_boot_transaction`, and `os/systemd/wros-offline-update.service`):**
  - Added `os/systemd/wros-offline-update.service` (linked into `system-update.target.wants` by `os/scripts/install-files.sh`), structured per `systemd.offline-updates(7)` (`DefaultDependencies=no`, `ConditionPathExists=/system-update`, `Environment=WROS_SYSTEMD_OFFLINE_UPDATE=1`, `ExecStart=/usr/libexec/wros-update run-offline-boot`, `TimeoutStartSec=1800s`, `SuccessAction=reboot`, `OnFailure=reboot.target`).
  - `prepare_offline_transaction` authenticates the bundle (`verify_bundle` with `dpkg_dir`), copies `.deb` archives into `/var/lib/wros-update/staged/<tx_id>/pool`, writes a durable `journal.json` (`status: "staged"`), and points `/system-update` to the staged directory.
  - `execute_offline_boot_transaction` removes `/system-update` **immediately before** running `apt-get` (preventing boot loops on failure), verifies all staged `.deb` SHA-256 digests against the journal, acquires POSIX `dpkg` locks (`lock-frontend` then `lock`), runs `apt-get --no-download --yes` under a dedicated 1800-second boot timeout, and updates `journal.json` to `applied` or `failed` (with rescue instructions surfaced in `inspect_recovery`).
  - Interactive mutations (`stage`, `cancel-staged`, `recover-dpkg`, or `run-offline-boot` without `WROS_SYSTEMD_OFFLINE_UPDATE=1` under PID 1) remain refused in `os/scripts/wros-update` and `crates/writingdeck-sys/src/linux.rs`.

## 2. Changed, Added, Deleted, and Renamed Files

### Added Files (6)
- `os/config/50-wros-installer.rules`: Polkit rule authorizing `org.wros.installer` for the live session user (`uid == 1000`) on `tty1` when `/run/live/medium` is present.
- `os/config/org.wros.installer.policy`: Polkit action definition for `/usr/libexec/wros-install-session`.
- `os/scripts/wros-install-session` (`0755`): Root supervisor and isolated mount-namespace worker for the live encrypted installer session.
- `os/systemd/wros-offline-update.service`: Systemd `system-update.target` unit for isolated offline update execution with an 1800-second timeout.
- `os/tests/test_install_session.py`: 7 unit tests exercising live-only refusal, single-instance lock, encrypted target proof validation, leaf-first target teardown, and worker lifecycle outcomes.
- `src/live_installer.rs`: Native Grimoire helper for launching and parsing `/usr/libexec/wros-install-session` via `pkexec` on `tty1`, with 8 unit tests.

### Modified Files (16)
- `ARGON_CHANGELOG.md`: Updated for the `4eeca44` continuation contribution.
- `docs/ARGON_INTEGRATION_2026-10-31.md`: Updated status and concrete code pointers for #3 and #5.
- `docs/FEATURE_MATRIX.md`: Synchronized capability rows for live installer TUI and signed updates.
- `docs/RELEASE_READINESS.md`: Updated boundary table for production installation (#3) and signed updates (#5).
- `docs/feature-status.json`: Synchronized machine-readable capability verification/detail strings with `docs/FEATURE_MATRIX.md`.
- `os/scripts/install-files.sh`: Enabled `wros-offline-update.service` in `system-update.target.wants`.
- `os/scripts/stage-live-session.sh`: Staged `wros-installer`, `wros-install-session`, `50-wros-installer.rules`, `org.wros.installer.policy`, and `tty7` service masks (`getty@tty7.service`, `autovt@tty7.service`).
- `os/scripts/wros-update`: Added `Date:`/`Valid-Until:` freshness checks, `dpkg --compare-versions` anti-downgrade and dependency/conflict checks, POSIX `F_GETLK`/`F_SETLK` lock inspection/acquisition, and durable offline transaction staging/execution (`prepare_offline_transaction`, `execute_offline_boot_transaction`) while preserving interactive mutation refusal.
- `os/tests/test_live_session.py`: Updated live-session fixture and added assertions for `wros-installer`, `wros-install-session`, Polkit installer rules, and `tty7` masks.
- `os/tests/test_update.py`: Added `test_freshness_anti_downgrade_dependency_and_posix_dpkg_lock_checks` and `test_offline_boot_transaction_removes_trigger_early_and_records_durable_journal`.
- `src/app/shell.rs`: Added `Route::InstallerConfirm` modal (defaulting to `Cancel`) and live-session installer entry flow.
- `src/app/writingdeck.rs`: Added `pending_live_installer`, `request_live_installer()` (enforcing `persist_dirty_buffers_for_power_action()`), `take_pending_live_installer()`, and `finish_live_installer()`.
- `src/main.rs`: Added `mod live_installer;` and the `tty1` `input.pause()` / `live_installer::run_live_installer()` / `input.resume(proof)` handoff loop.
- `src/shell_ui_tests.rs`: Added `live_installer_handoff_confirmation_and_dirty_buffer_gate_preserve_both_editors_at_four_sizes`.
- `tools/audit-live-session.py`: Audited `/usr/bin/wros-installer`, `/usr/libexec/wros-install-session`, `50-wros-installer.rules`, `org.wros.installer.policy`, and `tty7` masks.
- `tools/verify-writingdeck.py`: Added `os/tests/test_install_session.py` to the gate runner list.

### Deleted or Renamed Files
- None.

### Dependency & Lockfile Status
- `Cargo.toml` and `Cargo.lock` are completely unchanged (0 new crates, 0 version changes).
- `HANDOVER_BASELINE.json` and all historical files under `docs/evidence/` are preserved byte-for-byte.

## 3. Tests Added (Unrun vs Executed)

### Rust Tests Added (Unrun — No Rust Toolchain in Remote Environment)
1. `src/live_installer.rs` (`#[cfg(test)] mod tests`):
   - `parse_outcomes_from_tagged_output`
   - `nonzero_exit_without_failure_tag_maps_to_process_error`
   - `symlinked_or_group_writable_helper_is_refused`
   - `non_root_helper_owner_is_refused`
   - `handoff_invokes_pkexec_with_expected_helper_and_tty`
   - `installed_system_refusal_is_parsed_cleanly`
   - `busy_outcome_is_parsed_cleanly`
   - `cleanup_failed_outcome_is_parsed_cleanly`
2. `src/shell_ui_tests.rs`:
   - `live_installer_handoff_confirmation_and_dirty_buffer_gate_preserve_both_editors_at_four_sizes`

### Python Tests Added & Executed in Container
1. `os/tests/test_install_session.py` (7 tests, all passing):
   - `test_client_and_cleanup_outcomes_completed_cancelled_interrupted_and_unverified_refusal`
   - `test_live_input_reader_blocks_handoff_until_retired`
   - `test_service_roles_cannot_be_called_from_pkexec_or_ordinary_parent`
   - `test_session_must_match_the_live_callers_active_local_seat_and_tty1`
   - `test_shipping_preseed_matches_repo_and_refuses_disk_writes_or_credentials`
   - `test_storage_baseline_protects_live_media_and_cleans_target_lvm_luks_stack`
   - `test_systemd_command_contains_namespace_isolation_and_execstoppost_cleanup`
2. `os/tests/test_live_session.py` (11 tests, all passing):
   - Updated fixture and added verification that missing `usr/bin/wros-installer`, missing `usr/libexec/wros-install-session`, missing `50-wros-installer.rules`, or unmasked `getty@tty7.service` / `autovt@tty7.service` fail the live-session audit.
3. `os/tests/test_update.py` (7 tests, all passing):
   - Added `test_freshness_anti_downgrade_dependency_and_posix_dpkg_lock_checks` covering expired `Valid-Until`, future `Date`, `dpkg --compare-versions` downgrade refusal, held-package refusal, missing dependency refusal, `Conflicts` refusal, and POSIX `F_GETLK` lock detection.
   - Added `test_offline_boot_transaction_removes_trigger_early_and_records_durable_journal` covering `prepare_offline_transaction`, early `/system-update` removal before `apt-get` execution, failed transaction journal recording, and successful transaction application.

## 4. Exact Static & Python Checks Performed

All commands below were executed in `/working_dir/c_b9d01d08c565b2f9/work_4eeca44/WrOS` using `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp`:

1. `python3 tools/check-writingdeck.py`
   - Result: `PASS: 18 required docs + STATUS, local links, 89 capability rows, 40 lessons, content/dictionary hashes, notices, offline printable reference` (exit code `0`).
2. `python3 -m py_compile os/scripts/wros-install-session os/scripts/wros-update tools/audit-live-session.py tools/verify-writingdeck.py os/tests/test_install_session.py os/tests/test_live_session.py os/tests/test_update.py`
   - Result: exit code `0`.
3. `bash -n os/scripts/stage-live-session.sh os/scripts/install-files.sh`
   - Result: exit code `0`.
4. `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp python3 -m unittest discover -s os/tests -p 'test_install_session.py' -v`
   - Result: `Ran 7 tests ... OK` (exit code `0`).
5. `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp python3 -m unittest discover -s os/tests -p 'test_live_session.py' -v`
   - Result: `Ran 11 tests ... OK` (exit code `0`).
6. `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp python3 -m unittest discover -s os/tests -p 'test_update.py' -v`
   - Result: `Ran 7 tests ... OK` (exit code `0`).
7. `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp python3 -m unittest discover -s os/tests -p 'test_fixture_storage.py' -v`
   - Result: `Ran 7 tests ... OK` (exit code `0`).
8. `TMPDIR=/working_dir/c_b9d01d08c565b2f9/tmp python3 -m unittest discover -s os/tests -p 'test_native_image.py' -v`
   - Result: `Ran 4 tests ... OK` (exit code `0`).

## 5. Known Risks & Remaining Gaps

1. **Uncompiled Rust Changes:** `src/live_installer.rs`, `src/main.rs`, `src/app/writingdeck.rs`, `src/app/shell.rs`, and `src/shell_ui_tests.rs` have not been compiled with `cargo` or checked with `clippy` because no Rust toolchain exists in this environment. Run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo clippy --locked --no-default-features --all-targets -- -D warnings` on the Debian builder first.
2. **Disposable-QEMU Encrypted Install & First-Boot Qualification (#3):** While `os/scripts/wros-install-session` and `src/live_installer.rs` implement the production supervisor, target proof verification, and leaf-first teardown, full end-to-end encrypted installation and first boot on a freshly generated QEMU disk (`os/tests/native_installer_vm.py`) must be executed on the developer's machine before marking #3 Tested.
3. **Signed Update Mutations Still Refused Pending VM Interruption Qualification (#5):** `os/scripts/wros-update` and `os/systemd/wros-offline-update.service` implement bundle freshness, `dpkg --compare-versions` anti-downgrade/dependency/conflict checks, POSIX `F_GETLK`/`F_SETLK` lock coordination, early `/system-update` removal, and the durable transaction journal, while interactive mutations in `crates/writingdeck-sys/src/linux.rs` and `os/scripts/wros-update` CLI remain refused until disposable-VM interruption and rescue recovery are qualified.
