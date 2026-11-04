# Release readiness — 2026-10-31

The current candidate targets Debian 13 amd64. It adds a complete native
encrypted project backup/restore workflow, optional rclone Google Drive
connection, the local character-knowledge contribution, installer hardening,
protected native Wi-Fi enrollment, durable backup retry, and the supervised
production live-installer session handoff. The
[installer/update receiving review](ARGON_INSTALLER_UPDATE_REVIEW_2026-10-31.md)
records the corrected supervision, disk ownership and update planning. Full local
and independent CI software qualification passed; production/device/provider
boundaries remain explicit.
It remains a development candidate: a passing software suite does not make the
unqualified installation, update/recovery or physical-device work complete.

## Integrated work

- Native local/Drive repository setup, private recovery keys, masked Google
  authorization, full-project snapshots, actual restore/hash verification,
  snapshot selection, safe restore into new folders, cancellation and retry.
  See [workflow and limits](NATIVE_BACKUP.md).
- Character knowledge linked to reviewed facts, exact scene revisions and durable
  document identities, with retained history, filtering and source navigation.
- Local installer fixes from issues 70–74: password clearing on Back, offline
  service defaults, guarded media publication/kernel ABI checks, forced-reboot
  policy controls and LF/BOM enforcement. The original hardening branch is retained.
- Issue 69 helper/greeter fixes for preserved Debian options/models, refused
  preview recovery and stale rollback timers. Current-image and real-installer
  keyboard qualification remain separate.

The executed [software receipt](evidence/test-results.json) binds tests to source
and the compiled binary. Results from previous source versions remain historical;
only a fresh passing run establishes this candidate's software qualification.
The [complete current local run](evidence/installer-update-local.json) passed
1,101 Rust tests per overlapping configuration, 240 Python tests with zero skips,
both strict Clippys and 21 gates. [Independent CI](evidence/installer-update-ci.json)
passed the same test counts and both Clippys across 20 gates on the same source.
[Local provenance and native backup regression frames](evidence/installer-update-local-provenance.json)
retain the source/executable identities and four terminal sizes. These are
software receipts, not a full production installation or update-recovery certificate.

## Remaining release-critical work

| Work | Current boundary |
|---|---|
| [Production installation](https://github.com/j5cfk7qm49-web/WrOS/issues/3) | Production live-session installer supervisor (`os/scripts/wros-install-session`), Polkit live-user authorization (`os/config/50-wros-installer.rules`, `os/config/org.wros.installer.policy`), live-image staging/auditing (`os/scripts/stage-live-session.sh`, `tools/audit-live-session.py`), and native Grimoire console handoff (`src/live_installer.rs`, `Route::InstallerConfirm` in `src/app/shell.rs`) are implemented with owned leaf-first target LUKS2/LVM/swap/mount teardown and in-target encrypted proof verification. Exactly one eligible disk and a locked frontend identity are required. The [receiving review](ARGON_INSTALLER_UPDATE_REVIEW_2026-10-31.md) records the process/cleanup/proof corrections. Actual production handoff, full installation/first boot and physical qualification remain open. |
| [Signed updates and recovery](https://github.com/j5cfk7qm49-web/WrOS/issues/5) | Cached APT plans, read-only signed-bundle verification (`gpgv`), metadata freshness (`Date:` / `Valid-Until:`), `dpkg --compare-versions` anti-downgrade and dependency/conflict checks, POSIX `F_GETLK`/`F_SETLK` dpkg lock coordination, durable offline transaction journal (`prepare_offline_transaction` / `execute_offline_boot_transaction`), early `/system-update` trigger removal, and `os/systemd/wros-offline-update.service` are implemented. Real APT planning refuses removals/unreviewed packages; failed transactions retain rescue archives and foreign boot markers are preserved. Interactive root actions and automatic boot activation remain disabled pending native workflow, boot-time trust revalidation and disposable-VM interruption/rescue qualification. See [gap list and return order](ARGON_INTEGRATION_2026-10-31.md). |
| Native Wi-Fi enrollment | Protected new-network credential entry (`stdin` to `nmcli --ask`, redacted `Debug`/`Serialize`, volatile drop scrub), active-connection verification, disconnect, and UUID saved-profile reconnect/forget are implemented in source (Rust adapter and four-size native UI software checks passed; real NetworkManager/device acceptance pending). |
| [Input/lock/power](https://github.com/j5cfk7qm49-web/WrOS/issues/4) | Manual lock has prior qualification; software lock-before-sleep protocol (`evaluate_pre_sleep_lock` in `os/scripts/wros-lock`, `verify_pre_sleep_lock_protocol` in `src/app/writingdeck.rs`) is implemented and tested while hardware suspend/sleep remains disabled (`60-wros-sleep-guard.conf`, `SUSPEND_UNAVAILABLE`) per #32 pending VM/hardware qualification. |
| [Google account lifecycle](https://github.com/j5cfk7qm49-web/WrOS/issues/28) | Headless authorization import, private `google-account.json` lifecycle state, explicit verification (`check-google`), revocation vs offline classification, reauthorization and best-effort local credential removal (`disconnect-google`) are implemented. Live account acceptance and OS-wide secret vault remain open. |
| [Backup/sync](https://github.com/j5cfk7qm49-web/WrOS/issues/10) | Native encrypted snapshots, real local/rclone-fixture restore, durable offline backup queue (`queue.json`), interruption-safe deduplicating retry (`run-queue`), folder-scoped Google Drive browsing/materialization (`drive-list` / `drive-materialize`, `Route::GoogleDrive`), and per-book readable Google Drive sync (`wros-sync`: history in WrOS-History, paragraph merge, delete guard) are implemented and tested (`test_backup.py`, `test_sync.py`, Rust UI tests). Live Drive restoration and unqualified provider round trips remain open. |
| [Hardware and release qualification](https://github.com/j5cfk7qm49-web/WrOS/issues/13) | Target-device boot, Wi-Fi, keyboard, battery and power checks, release signing and corresponding-source/notices qualification remain required. |

General Drive browsing, optional mobile workflows and other roadmap expansion
should follow the dependencies above. Preserve the working writer and backup
path while closing these gaps; do not relabel a VM demo as a production OS.
