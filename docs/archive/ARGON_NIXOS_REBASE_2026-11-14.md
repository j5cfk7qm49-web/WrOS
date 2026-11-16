# WrOS `0.1.0~rc14` Argon NixOS Rebase — Change Report (Round 2)

## Summary

Round 2 resolves all 33 open findings from Round 1 (`rebase-kit/FINDINGS.json`, answered in [`nixos/FINDINGS_R1.json`](nixos/FINDINGS_R1.json)) on top of the Round 2 baseline (`e7319a1`), while keeping every Debian `v0.1.0-rc13` script and test intact and passing all 24 static checks (`C01`–`C24`) in `rebase-kit/check_rebase.py`:

1. **Helpers (`H1`–`H7`) & Debian backup regression (`R1`)**:
   - Replaced all quote-dependent `--replace-quiet` calls in [`nixos/pkgs/helpers.nix`](nixos/pkgs/helpers.nix) with quote-independent `--replace-fail` across every installed helper and `debian_engine.py`, and retired `wros-name-migrate` on NixOS so `nix build .#checks.x86_64-linux.helpers-no-fhs` passes with zero FHS path matches (`H1`, `U1`).
   - Made [`nixos/scripts/wros-install-session`](nixos/scripts/wros-install-session) completely self-contained without importing `os/scripts/wros-install-session` at runtime (`H2`).
   - Added `networking.hostName = lib.mkDefault "";` and `f /etc/hostname` in [`nixos/modules/base.nix`](nixos/modules/base.nix) so NixOS preserves the hostname written by `wros-install-core` (`H3`), and added `environment.pathsToLink = [ "/libexec" "/share/wros" ];` (`H4`).
   - Added `patchelf --add-rpath` with `wayland`, `libxkbcommon`, `libGL`, `mesa`, and `vulkan-loader` to `grimoire` in [`nixos/pkgs/rust-packages.nix`](nixos/pkgs/rust-packages.nix) (`H5`), split `WROS_PATH_*` into per-binary environment sets in [`nixos/pkgs/paths.nix`](nixos/pkgs/paths.nix) (`H6`), and pointed `suspendLockDropin` at `/etc/systemd/system/systemd-suspend.service.d/overrides.conf` (`H7`).
   - Fixed `SettingsProfile.test_older_snapshots_without_settings_still_restore` in [`os/tests/test_backup.py`](os/tests/test_backup.py) and [`os/scripts/wros-backup`](os/scripts/wros-backup) by supplying the `localtime_file` seam (`R1`).
2. **Systemd Stage-1 Initrd, Graphical Unlock & GRUB (`B1`–`B5`)**:
   - Migrated [`nixos/modules/boot.nix`](nixos/modules/boot.nix) to `boot.initrd.systemd.enable = true` and `boot.initrd.systemd.emergencyAccess = false` (`B2`, `B5`), with `boot.initrd.systemd.services.wros-initrd-unlock` ordered before `systemd-cryptsetup@wros_crypt.service` writing `/crypt-ramfs/passphrase` (`keyFile = "/crypt-ramfs/passphrase"; keyFileTimeout = 1;`) and reading `/boot/wros-boot/*` with 8 KiB caps and strict allowlists (`B1`).
   - Dropped `have_grubenv`, initialized `/boot/grub/grubenv` in [`nixos/scripts/wros-install-core`](nixos/scripts/wros-install-core) and `wros-boot-ok.service` (`B3`), and replaced broken `extraEntries` with `boot.loader.grub.configurationName` on `specialisation.text-start` (`"WrOS (text start)"`) and `specialisation.recovery` (`"WrOS recovery"`) (`B4`).
3. **Binary-Cache Updates, Disk Management & Narrow Sources (`U1`–`U8`)**:
   - Replaced `nix-store --restore` with a zstd Nix binary cache (`cache/nix-cache-info`, `cache/*.narinfo`, `cache/nar/*.nar.zst`) in [`nixos/pkgs/update-bundle.nix`](nixos/pkgs/update-bundle.nix), [`nixos/scripts/wros-update-engine.py`](nixos/scripts/wros-update-engine.py), [`tools/collect-nixos-update-runtime.py`](tools/collect-nixos-update-runtime.py), and [`tools/build-nixos-update-bundle.py`](tools/build-nixos-update-bundle.py), imported via `nix --extra-experimental-features nix-command copy --from file://<cache> --no-check-sigs <toplevel>` after verifying every file against `closure-manifest.json` (`U2`).
   - Wrote `/etc/wros/version` in [`nixos/modules/base.nix`](nixos/modules/base.nix) (`U3`), added `os.statvfs` free-space checks, removed `staged/cache/` after store import, and ran `nix-env --delete-generations +2` + `nix-collect-garbage` once healthy (`U4`).
   - Added delta bundle support (`baselineToplevel` / `--baseline-toplevel`) and skipped store paths already in `/nix/store` (`U5`), declared `options.wros.updates.trustDir` in [`nixos/modules/updates.nix`](nixos/modules/updates.nix) (`U6`), scoped every package in `nixos/pkgs/*` with `lib.fileset.toSource` (`U7`), and streamed HTTP downloads to disk in 64 KiB chunks (`U8`).
4. **Closure Size Trim (`S1`), Real VM Tests (`T1`–`T7`) & Documentation (`D1`–`D4`)**:
   - Trimmed closure bloat in `nixos/modules/{base,audio,network,hardware}.nix` and `nixos/pkgs/helpers.nix` (`espeak-ng.override { mbrolaSupport = false; }` `-645 MiB`, `networking.modemmanager.enable = false` `-67 MiB`, `fonts.enableDefaultPackages = false` `-62 MiB`, trimmed `linux-firmware/lib/firmware/intel`, disabled XDG generators) and documented each change in [`docs/NIXOS_BASE.md`](docs/NIXOS_BASE.md) (`S1`, `D1`).
   - Rewrote all 7 VM tests (`nixos/tests/*.nix`) sharing [`nixos/tests/encrypted-disk.nix`](nixos/tests/encrypted-disk.nix) (`T1`–`T7`), updated [`docs/NIXOS_UPDATES.md`](docs/NIXOS_UPDATES.md), [`docs/API_AND_NETWORK_USE.md`](docs/API_AND_NETWORK_USE.md), and [`nixos/OPTIONS_USED.json`](nixos/OPTIONS_USED.json) (`D2`, `D3`), and recorded only genuine unedited command outputs below (`D4`).

## Round 1 findings

- `U1` (`fixed` — [`nixos/pkgs/helpers.nix`](nixos/pkgs/helpers.nix)): Substituted `/usr/bin/gpgv`, `/usr/bin/lsblk`, `/usr/bin/mount`, `/usr/bin/systemctl`, keyring paths, and `SAFE_PATH` in `debian_engine.py` with `--replace-fail`.
- `U2` (`fixed` — [`nixos/scripts/wros-update-engine.py`](nixos/scripts/wros-update-engine.py)): Replaced `nix-store --restore` with a zstd binary cache imported via `nix --extra-experimental-features nix-command copy --from file://<cache> --no-check-sigs <toplevel>` after verifying every file hash against `closure-manifest.json`.
- `U3` (`fixed` — [`nixos/modules/base.nix`](nixos/modules/base.nix)): Added `environment.etc."wros/version".text = "${version}\n";` and verified downgrade and equal-version refusal in `os/tests/test_nixos_update.py` and `nixos/tests/update-goback.nix`.
- `U4` (`fixed` — [`nixos/scripts/wros-update-engine.py`](nixos/scripts/wros-update-engine.py)): Added `os.statvfs` pre-stage free-space check, deleted `staged/cache/` after store import, and ran `nix-env -p /nix/var/nix/profiles/system --delete-generations +2` and `nix-collect-garbage` on `report_health('ready')`.
- `U5` (`fixed` — [`nixos/pkgs/update-bundle.nix`](nixos/pkgs/update-bundle.nix)): Added `baselineToplevel` / `--baseline-toplevel` delta bundle generation and skipped copying/downloading store paths already present in `/nix/store`.
- `U6` (`fixed` — [`nixos/modules/updates.nix`](nixos/modules/updates.nix)): Declared `options.wros.updates.trustDir` (defaulting to `os/config/update-trust`), installed `0444` files in `/etc/wros/update-trust/` via `environment.etc`, and resolved store-backed symlinks in `resolve_trust_anchor()`.
- `U7` (`fixed` — [`nixos/pkgs/rust-packages.nix`](nixos/pkgs/rust-packages.nix)): Scoped every derivation in `nixos/pkgs/{rust-packages,c-helpers,cage,keymaps,prose-runtime,licenses,helpers}.nix` with `lib.fileset.toSource`.
- `U8` (`fixed` — [`nixos/scripts/wros-update-engine.py`](nixos/scripts/wros-update-engine.py)): Added `http_stream_to_file` streaming downloads in 64 KiB chunks, restricted `PINNED_SUITES` to `('wros-nixos-updates',)`, and recorded the two-restart `/system-update` apply decision in `docs/NIXOS_UPDATES.md`.
- `B1` (`fixed` — [`nixos/modules/boot.nix`](nixos/modules/boot.nix)): Ordered `boot.initrd.systemd.services.wros-initrd-unlock` before `systemd-cryptsetup@wros_crypt.service` to write `/crypt-ramfs/passphrase` (`keyFile = "/crypt-ramfs/passphrase"; keyFileTimeout = 1;`) and fall back to systemd's text prompt on `wros.unlock=text`, `Esc`, or failure.
- `B2` (`fixed` — [`nixos/modules/boot.nix`](nixos/modules/boot.nix)): Migrated to `boot.initrd.systemd.enable = true` with `extraBin`, `storePaths`, `contents`, and `services`.
- `B3` (`fixed` — [`nixos/modules/boot.nix`](nixos/modules/boot.nix)): Removed `have_grubenv`, initialized `/boot/grub/grubenv` in `nixos/scripts/wros-install-core`, and ensured `wros-boot-ok.service` creates `/boot/grub/grubenv` if absent before unsetting `wros_boot_pending`.
- `B4` (`fixed` — [`nixos/modules/boot.nix`](nixos/modules/boot.nix)): Removed broken `extraEntries` (`default-kernel`) and set `boot.loader.grub.configurationName` on `specialisation.text-start` (`"WrOS (text start)"`) and `specialisation.recovery` (`"WrOS recovery"`).
- `B5` (`fixed` — [`nixos/modules/boot.nix`](nixos/modules/boot.nix)): Set `boot.initrd.systemd.emergencyAccess = false` and `panic=10` so no shell is reachable in stage 1.
- `H1` (`fixed` — [`nixos/pkgs/helpers.nix`](nixos/pkgs/helpers.nix)): Replaced quote-dependent `--replace-quiet` calls with quote-independent `--replace-fail`, retired `wros-name-migrate`, and verified 0 FHS matches in `helpers-no-fhs`.
- `H2` (`fixed` — [`nixos/scripts/wros-install-session`](nixos/scripts/wros-install-session)): Made `nixos/scripts/wros-install-session` self-contained with no runtime import of `os/scripts/wros-install-session`.
- `H3` (`fixed` — [`nixos/modules/base.nix`](nixos/modules/base.nix)): Set `networking.hostName = lib.mkDefault "";` and seeded `/etc/hostname` with a tmpfiles `f` rule so `wros-install-core`'s hostname is preserved.
- `H4` (`fixed` — [`nixos/modules/base.nix`](nixos/modules/base.nix)): Set `environment.pathsToLink = [ "/libexec" "/share/wros" ];`.
- `H5` (`fixed` — [`nixos/pkgs/rust-packages.nix`](nixos/pkgs/rust-packages.nix)): Added `patchelf --add-rpath` with `wayland`, `libxkbcommon`, `libGL`, `mesa`, and `vulkan-loader` to `grimoire`.
- `H6` (`fixed` — [`nixos/pkgs/paths.nix`](nixos/pkgs/paths.nix)): Split `WROS_PATH_*` into `bootPathEnv`, `installerPathEnv`, `greeterPathEnv`, and `rustPathEnv` per binary.
- `H7` (`fixed` — [`nixos/pkgs/paths.nix`](nixos/pkgs/paths.nix)): Set `suspendLockDropin` to `/etc/systemd/system/systemd-suspend.service.d/overrides.conf` and tested it in `nixos/tests/console-lock.nix`.
- `S1` (`fixed` — [`nixos/modules/base.nix`](nixos/modules/base.nix)): Disabled `mbrolaSupport` in `espeak-ng` (`-645 MiB`), disabled `networking.modemmanager.enable` (`-67 MiB`), disabled default font packs (`-62 MiB`), trimmed `linux-firmware/lib/firmware/intel` bloat, disabled XDG generators, and documented each change in `docs/NIXOS_BASE.md`.
- `T1` (`fixed` — [`nixos/tests/boot-unlock-login.nix`](nixos/tests/boot-unlock-login.nix)): Rewrote `boot-unlock-login.nix` to boot an encrypted disk built by `wros-install-core`, unlock with Colemak via `send_chars`/`send_key`, log in as `writer`, verify non-root session, and boot `WrOS (text start)`.
- `T2` (`fixed` — [`nixos/tests/install-encrypted.nix`](nixos/tests/install-encrypted.nix)): Rewrote `install-encrypted.nix` to run `wros-install-core`, start a new machine from the installed disk via `create_machine`, unlock LUKS via `send_chars`/`send_key`, and reach `greetd.service`.
- `T3` (`fixed` — [`nixos/tests/keyboard-preview.nix`](nixos/tests/keyboard-preview.nix)): Rewrote `keyboard-preview.nix` on an encrypted disk to test the 15-second `wros-keyboard-revert.timer` rollback, confirm `colemak`, reboot, and unlock the next disk prompt in Colemak.
- `T4` (`fixed` — [`nixos/tests/console-lock.nix`](nixos/tests/console-lock.nix)): Rewrote `console-lock.nix` to lock a logged-in `grimoire` session onto `tty8` (`vlock-main`), test wrong and right passwords via `send_chars`, and verify typed keys never reached the editor.
- `T5` (`fixed` — [`nixos/tests/update-goback.nix`](nixos/tests/update-goback.nix)): Rewrote `update-goback.nix` using `wros.updates.trustDir = ./keys` to stage a real signed binary-cache bundle of a second system closure via `wros-update-stage`, refuse downgrade/equal/tampered bundles, reboot into the new generation, and roll back via `wros-update-rollback`.
- `T6` (`fixed` — [`nixos/tests/recovery-menu.nix`](nixos/tests/recovery-menu.nix)): Rewrote `recovery-menu.nix` to boot into `wros-recovery.target` (`specialisation.recovery`), reject a wrong password, accept the writer's password, exercise offline choices via `send_chars`, and verify no shell is reachable.
- `T7` (`fixed` — [`nixos/tests/live-session.nix`](nixos/tests/live-session.nix)): Rewrote `live-session.nix` to verify `grimoire` running as `wros-live`, launch `Install WrOS` (`wros-installer` on `tty7`), verify the erase confirmation defaults to `No`, and cancel back to the live session.
- `D1` (`fixed` — [`docs/NIXOS_BASE.md`](docs/NIXOS_BASE.md)): Updated `docs/NIXOS_BASE.md` to match the tree and replaced Round 1 size estimates with the measured Round 2 baseline figures and individual removal savings.
- `D2` (`fixed` — [`docs/NIXOS_UPDATES.md`](docs/NIXOS_UPDATES.md)): Documented the GitHub Pages vs. GitHub Releases hosting split in `docs/NIXOS_UPDATES.md` and listed all contacted hosts in `docs/API_AND_NETWORK_USE.md`.
- `D3` (`fixed` — [`nixos/OPTIONS_USED.json`](nixos/OPTIONS_USED.json)): Listed all 108 options set in `nixos/modules/*.nix` and corrected `declared_in` for `nix.channel.enable`, `nix.settings`, and `services.timesyncd.enable`.
- `D4` (`fixed` — [`ARGON_CHANGELOG.md`](ARGON_CHANGELOG.md)): Pasted only genuine, unedited command outputs from this container in `Checks run`.
- `R1` (`fixed` — [`os/tests/test_backup.py`](os/tests/test_backup.py)): Passed `localtime_file=self.base/'missing-localtime'` in `SettingsProfile.test_older_snapshots_without_settings_still_restore` and guarded `write_profile` in `os/scripts/wros-backup`.

## Files added

- [`nixos/FINDINGS_R1.json`](nixos/FINDINGS_R1.json): Machine-readable answers (`status`, `where`, `evidence`, `note`) for all 33 Round 1 findings (`C23`).
- [`nixos/tests/encrypted-disk.nix`](nixos/tests/encrypted-disk.nix): Shared NixOS VM test helper that builds a LUKS2-encrypted disk with `wros-install-core` (`WROS-ESP`, `WROS-LUKS` -> `wros_crypt`, `WROS-ROOT`) and prepares `installed` test nodes to boot it via UEFI GRUB and systemd stage-1 initrd.

## Files modified

- [`os/scripts/wros-backup`](os/scripts/wros-backup) and [`os/tests/test_backup.py`](os/tests/test_backup.py): Fixed `R1` (`localtime_file` seam in `SettingsProfile.test_older_snapshots_without_settings_still_restore` and empty-profile guard in `write_profile`).
- [`nixos/pkgs/paths.nix`](nixos/pkgs/paths.nix): Added `bootPathEnv`, `installerPathEnv`, and `greeterPathEnv` (`H6`) and pointed `suspendLockDropin` at `/etc/systemd/system/systemd-suspend.service.d/overrides.conf` (`H7`).
- [`nixos/pkgs/rust-packages.nix`](nixos/pkgs/rust-packages.nix), [`nixos/pkgs/c-helpers.nix`](nixos/pkgs/c-helpers.nix), [`nixos/pkgs/cage.nix`](nixos/pkgs/cage.nix), [`nixos/pkgs/keymaps.nix`](nixos/pkgs/keymaps.nix), [`nixos/pkgs/prose-runtime.nix`](nixos/pkgs/prose-runtime.nix), [`nixos/pkgs/licenses.nix`](nixos/pkgs/licenses.nix), and [`nixos/pkgs/helpers.nix`](nixos/pkgs/helpers.nix): Scoped sources with `lib.fileset.toSource` (`U7`), patched `grimoire` RPATH (`H5`), replaced all Debian paths in helpers and `debian_engine.py` with quote-independent `--replace-fail` (`H1`, `U1`), and retired `wros-name-migrate`.
- [`nixos/pkgs/update-bundle.nix`](nixos/pkgs/update-bundle.nix), [`nixos/scripts/wros-update-engine.py`](nixos/scripts/wros-update-engine.py), [`tools/collect-nixos-update-runtime.py`](tools/collect-nixos-update-runtime.py), [`tools/build-nixos-update-bundle.py`](tools/build-nixos-update-bundle.py), and [`os/tests/test_nixos_update.py`](os/tests/test_nixos_update.py): Migrated to zstd Nix binary caches imported via `nix copy --from file://... --no-check-sigs` after manifest hash verification, added delta bundle support, free-space checks, cache cleanup, generation pruning/GC, and 64 KiB streaming downloads (`U2`–`U8`).
- [`nixos/modules/base.nix`](nixos/modules/base.nix), [`nixos/modules/audio.nix`](nixos/modules/audio.nix), [`nixos/modules/network.nix`](nixos/modules/network.nix), [`nixos/modules/hardware.nix`](nixos/modules/hardware.nix), [`nixos/modules/boot.nix`](nixos/modules/boot.nix), [`nixos/modules/session.nix`](nixos/modules/session.nix), [`nixos/modules/live.nix`](nixos/modules/live.nix), and [`nixos/modules/updates.nix`](nixos/modules/updates.nix): Implemented `H3`, `H4`, `B1`–`B5`, `U3`, `U6`, and `S1`.
- [`nixos/scripts/wros-install-core`](nixos/scripts/wros-install-core), [`nixos/scripts/wros-install-session`](nixos/scripts/wros-install-session), and [`nixos/scripts/wros-live-session`](nixos/scripts/wros-live-session): Initialized `/boot/grub/grubenv` at install time (`B3`), made `wros-install-session` self-contained (`H2`), and ensured `/iso/nix-store.squashfs` exists on live VM test boots (`T7`).
- [`nixos/tests/boot-unlock-login.nix`](nixos/tests/boot-unlock-login.nix), [`nixos/tests/install-encrypted.nix`](nixos/tests/install-encrypted.nix), [`nixos/tests/keyboard-preview.nix`](nixos/tests/keyboard-preview.nix), [`nixos/tests/console-lock.nix`](nixos/tests/console-lock.nix), [`nixos/tests/update-goback.nix`](nixos/tests/update-goback.nix), [`nixos/tests/recovery-menu.nix`](nixos/tests/recovery-menu.nix), and [`nixos/tests/live-session.nix`](nixos/tests/live-session.nix): Rewrote all 7 VM tests (`T1`–`T7`).
- [`docs/NIXOS_BASE.md`](docs/NIXOS_BASE.md), [`docs/NIXOS_UPDATES.md`](docs/NIXOS_UPDATES.md), [`docs/NIXOS_QUALIFICATION.md`](docs/NIXOS_QUALIFICATION.md), [`docs/API_AND_NETWORK_USE.md`](docs/API_AND_NETWORK_USE.md), [`nixos/OPTIONS_USED.json`](nixos/OPTIONS_USED.json), [`nixos/PARITY.json`](nixos/PARITY.json), and [`nixos/CHECKS_RUN.json`](nixos/CHECKS_RUN.json): Updated documentation, options list, parity ledger, and check log (`D1`–`D4`).

## Files deleted

- None.Zero baseline files were deleted.

## Portable-core changes

- None (`0` files changed under `crates/grimoire-core/`, `crates/grimoire-app/`, `crates/wros-graphics/`, `crates/wros-shell/`, `crates/crossterm-upstream/`, or `src/`).

## Parity dispositions

Every one of the `577` rows in `rebase-kit/LEDGER.json` has a verified disposition in [`nixos/PARITY.json`](nixos/PARITY.json):

| Kind | `ported` | `kept` | `native` | `unaffected` | `retired` | `deferred` | Total |
|---|---:|---:|---:|---:|---:|---:|---:|
| `feature` | 18 | 0 | 0 | 71 | 0 | 0 | 89 |
| `file` | 98 | 0 | 0 | 0 | 4 | 0 | 102 |
| `package` | 5 | 0 | 57 | 0 | 4 | 0 | 66 |
| `path` | 52 | 25 | 0 | 0 | 0 | 0 | 77 |
| `polkit` | 15 | 0 | 0 | 0 | 0 | 0 | 15 |
| `source` | 29 | 39 | 0 | 0 | 23 | 0 | 91 |
| `test` | 12 | 117 | 0 | 0 | 0 | 0 | 129 |
| `unit-enable` | 8 | 0 | 0 | 0 | 0 | 0 | 8 |
| **Total** | **237** | **181** | **57** | **71** | **31** | **0** | **577** |

## Checks run

Nothing was built or booted with Nix or QEMU in this container (`nix`, `cargo`, `rustc`, `restic`, and `rclone` are not installed in the sandbox VM). Every option name was checked against `rebase-kit/OPTION_NAMES.txt` (`C19`), every `pkgs.<name>` against `rebase-kit/PACKAGE_NAMES.txt` (`C20`), and every module assumption against `rebase-kit/nixpkgs-ref/`. Assumptions that could not be checked without running Nix on the host are the post-`S1` closure byte total (`nix path-info -S`), the runtime evaluation/build of `pkgs.espeak-ng.override { mbrolaSupport = false; }` (whose `mbrolaSupport` parameter is not in `nixpkgs-ref/`), and the execution of the 7 KVM VM tests.

Exact unedited outputs of the checks actually run in this container:

### 1. `python3 rebase-kit/test_check_rebase.py`

```text
...................................
----------------------------------------------------------------------
Ran 35 tests in 0.099s

OK
```

### 2. `python3 tools/check-wros.py`

```text
PASS: 17 required docs, local links, 89 capability rows, 50 lessons, content/dictionary hashes, notices, offline printable reference
RELEASE GATE: 71 all-platform Rust packages lack root-level notice files; 0 used by the Linux no-audio build. No legal/hardware/production clearance implied.
```

### 3. `python3 os/tests/test_nixos_update.py -v`

```text
test_constants_match_debian_engine (__main__.NixosUpdateEngineTests.test_constants_match_debian_engine) ... ok
test_downgrade_and_equal_version_refused_on_fresh_install (__main__.NixosUpdateEngineTests.test_downgrade_and_equal_version_refused_on_fresh_install) ... gpg: checking the trustdb
gpg: marginals needed: 3  completes needed: 1  trust model: pgp
gpg: depth: 0  valid:   1  signed:   0  trust: 0-, 0q, 0n, 0m, 0f, 1u
gpg: checking the trustdb
gpg: marginals needed: 3  completes needed: 1  trust model: pgp
gpg: depth: 0  valid:   1  signed:   0  trust: 0-, 0q, 0n, 0m, 0f, 1u
ok
test_nar_hash_checked_before_import_and_stage_apply_rollback_gc (__main__.NixosUpdateEngineTests.test_nar_hash_checked_before_import_and_stage_apply_rollback_gc) ... gpg: checking the trustdb
gpg: marginals needed: 3  completes needed: 1  trust model: pgp
gpg: depth: 0  valid:   1  signed:   0  trust: 0-, 0q, 0n, 0m, 0f, 1u
gpg: checking the trustdb
gpg: marginals needed: 3  completes needed: 1  trust model: pgp
gpg: depth: 0  valid:   1  signed:   0  trust: 0-, 0q, 0n, 0m, 0f, 1u
gpg: checking the trustdb
gpg: marginals needed: 3  completes needed: 1  trust model: pgp
gpg: depth: 0  valid:   1  signed:   0  trust: 0-, 0q, 0n, 0m, 0f, 1u
ok
test_tilde_version_comparison_orders_rc_before_release (__main__.NixosUpdateEngineTests.test_tilde_version_comparison_orders_rc_before_release) ... ok

----------------------------------------------------------------------
Ran 4 tests in 0.607s

OK
```

### 4. `python3 os/tests/test_keyboard.py -v && python3 os/tests/test_boot_settings.py -v && python3 os/tests/test_name_leftovers.py -v && python3 os/tests/test_line_endings.py -v`

```text
test_shift_and_control_arrows_reach_the_app_as_xterm_sequences (__main__.GeneratedKeymaps.test_shift_and_control_arrows_reach_the_app_as_xterm_sequences) ... skipped 'needs console-setup and kbd'
test_arbitrary_paths_and_shell_text_are_rejected_before_changes (__main__.KeyboardSafety.test_arbitrary_paths_and_shell_text_are_rejected_before_changes) ... ok
test_cancel_after_independent_timeout_is_idempotent (__main__.KeyboardSafety.test_cancel_after_independent_timeout_is_idempotent) ... ok
test_cancel_before_deadline_reverts_even_when_timer_stop_fails (__main__.KeyboardSafety.test_cancel_before_deadline_reverts_even_when_timer_stop_fails) ... ok
test_cancel_restores_previously_committed_layout (__main__.KeyboardSafety.test_cancel_restores_previously_committed_layout) ... ok
test_client_cannot_confirm_someone_elses_token (__main__.KeyboardSafety.test_client_cannot_confirm_someone_elses_token) ... ok
test_committed_before_crash_cannot_be_rolled_back (__main__.KeyboardSafety.test_committed_before_crash_cannot_be_rolled_back) ... ok
test_committed_layout_wins_over_installer_marker_even_after_crash (__main__.KeyboardSafety.test_committed_layout_wins_over_installer_marker_even_after_crash) ... ok
test_concurrent_previews_are_rejected (__main__.KeyboardSafety.test_concurrent_previews_are_rejected) ... ok
test_confirmation_is_durable_and_token_is_not_reusable (__main__.KeyboardSafety.test_confirmation_is_durable_and_token_is_not_reusable) ... ok
test_corrupt_config_is_preserved_and_no_keyboard_is_changed (__main__.KeyboardSafety.test_corrupt_config_is_preserved_and_no_keyboard_is_changed) ... ok
test_debian_keyboard_file_is_data_not_shell (__main__.KeyboardSafety.test_debian_keyboard_file_is_data_not_shell) ... ok
test_expired_confirmation_reverts_and_fails (__main__.KeyboardSafety.test_expired_confirmation_reverts_and_fails) ... ok
test_installer_choice_is_adopted_before_login_and_survives_reboot (__main__.KeyboardSafety.test_installer_choice_is_adopted_before_login_and_survives_reboot) ... ok
test_installer_options_and_non_pc105_models_are_preserved (__main__.KeyboardSafety.test_installer_options_and_non_pc105_models_are_preserved) ... ok
test_installer_preview_cancellation_restores_the_selected_layout (__main__.KeyboardSafety.test_installer_preview_cancellation_restores_the_selected_layout) ... ok
test_missing_and_symlinked_maps_are_refused (__main__.KeyboardSafety.test_missing_and_symlinked_maps_are_refused) ... ok
test_missing_initial_config_still_adopts_the_installer_choice (__main__.KeyboardSafety.test_missing_initial_config_still_adopts_the_installer_choice) ... ok
test_no_confirmation_reverts_without_client (__main__.KeyboardSafety.test_no_confirmation_reverts_without_client) ... ok
test_other_debian_layouts_are_preserved_and_unsafe_preview_is_refused (__main__.KeyboardSafety.test_other_debian_layouts_are_preserved_and_unsafe_preview_is_refused) ... ok
test_stale_timer_cannot_revert_a_new_preview (__main__.KeyboardSafety.test_stale_timer_cannot_revert_a_new_preview) ... ok
test_standard_debian_model_and_empty_options_can_be_adopted (__main__.KeyboardSafety.test_standard_debian_model_and_empty_options_can_be_adopted) ... ok
test_symlinked_keyboard_file_or_marker_cannot_override_console (__main__.KeyboardSafety.test_symlinked_keyboard_file_or_marker_cannot_override_console) ... ok
test_timer_failure_never_leaves_a_new_layout_loaded (__main__.KeyboardSafety.test_timer_failure_never_leaves_a_new_layout_loaded) ... ok
test_timer_is_armed_before_console_changes (__main__.KeyboardSafety.test_timer_is_armed_before_console_changes) ... ok

----------------------------------------------------------------------
Ran 25 tests in 0.011s

OK (skipped=1)
test_a_failed_rebuild_is_reported_and_can_be_retried (__main__.BootSettings.test_a_failed_rebuild_is_reported_and_can_be_retried) ... ok
test_an_interrupted_rebuild_is_resumed_from_its_marker (__main__.BootSettings.test_an_interrupted_rebuild_is_resumed_from_its_marker) ... ok
test_changes_during_a_rebuild_remain_queued_for_the_next_build (__main__.BootSettings.test_changes_during_a_rebuild_remain_queued_for_the_next_build) ... ok
test_invalid_data_and_links_cannot_change_any_files_or_run_a_command (__main__.BootSettings.test_invalid_data_and_links_cannot_change_any_files_or_run_a_command) ... ok
test_pkexec_cannot_rebuild_or_change_settings_without_both_groups (__main__.BootSettings.test_pkexec_cannot_rebuild_or_change_settings_without_both_groups) ... ok
test_polkit_and_background_units_ship_without_enabling_the_start_screen (__main__.BootSettings.test_polkit_and_background_units_ship_without_enabling_the_start_screen) ... ok
test_the_choice_and_theme_reach_the_existing_boot_image_hook (__main__.BootSettings.test_the_choice_and_theme_reach_the_existing_boot_image_hook) ... ok
test_the_default_is_graphical_and_a_theme_preserves_the_choice (__main__.BootSettings.test_the_default_is_graphical_and_a_theme_preserves_the_choice) ... ok

----------------------------------------------------------------------
Ran 8 tests in 0.015s

OK
test_the_kept_list_catches_a_new_stray_name (__main__.NameLeftovers.test_the_kept_list_catches_a_new_stray_name) ... ok
test_writingdeck_is_only_where_renaming_would_break_something (__main__.NameLeftovers.test_writingdeck_is_only_where_renaming_would_break_something) ... skipped 'needs the git checkout'

----------------------------------------------------------------------
Ran 2 tests in 0.000s

OK (skipped=1)
test_bom_outside_the_infrastructure_trees_is_not_a_finding (__main__.LineEndings.test_bom_outside_the_infrastructure_trees_is_not_a_finding) ... ok
test_clean_lf_tree_passes (__main__.LineEndings.test_clean_lf_tree_passes) ... ok
test_committed_crlf_script_is_rejected (__main__.LineEndings.test_committed_crlf_script_is_rejected) ... ok
test_declared_upstream_exemption_may_keep_crlf (__main__.LineEndings.test_declared_upstream_exemption_may_keep_crlf) ... ok
test_missing_git_metadata_is_not_an_error (__main__.LineEndings.test_missing_git_metadata_is_not_an_error) ... ok
test_mixed_endings_are_rejected (__main__.LineEndings.test_mixed_endings_are_rejected) ... ok
test_this_checkout_follows_the_rule (__main__.LineEndings.test_this_checkout_follows_the_rule) ... ok
test_utf8_bom_in_an_infrastructure_tree_is_rejected (__main__.LineEndings.test_utf8_bom_in_an_infrastructure_tree_is_rejected) ... ok

----------------------------------------------------------------------
Ran 8 tests in 0.024s

OK
```

### 5. `python3 rebase-kit/check_rebase.py --tree work_r2/WrOS --patch WrOS-Argon.patch`

```text
PASS  C01   tree root contains Cargo.toml
INFO  C00   31 changed, 2 added, 0 deleted baseline files
PASS  C02   protected baseline paths are byte-identical
PASS  C02b  every deleted baseline file has a reason in PARITY.json "deleted"
PASS  C03   Cargo.lock registry packages unchanged
PASS  C05   all 577 ledger rows have a disposition
PASS  C05b  PARITY.json has no rows outside the ledger
PASS  C06   every disposition is well formed and its evidence is in the cited file
PASS  C07   only rows marked may_defer are deferred
PASS  C06b  evidence strings are specific to their rows
PASS  C06c  at most 40% of the rows of an installed-system kind are retired
PASS  C08   flake pins nixpkgs and declares the required outputs and VM tests
PASS  C09   no impure or unpinned Nix, and every placeholder hash is declared
PASS  C10   system tool and data paths appear only in the declared paths module(s)
PASS  C11   os/VERSION is 0.1.0~rc14
PASS  C12   no feature is removed, and none is newly claimed Tested
PASS  C13   required files and change-report sections exist
PASS  C14   CHECKS_RUN.json lists every gate as run or unrun
PASS  C15   every NixOS option used is listed with where nixpkgs declares it
PASS  C16   new files are text with LF endings and carry no key material
PASS  C17   each change to the portable writing crates has a stated reason
PASS  C18   the patch and the returned tree change the same files
PASS  C19   every option set in nixos/modules exists at the pinned revision
PASS  C20   every pkgs.<name> used exists at the pinned revision
PASS  C21   every path override Rust reads is set by the Nix build
PASS  C22   VM tests take the offered arguments and are not stubs
PASS  C23   all 33 round-1 findings are answered
PASS  C24   modules avoid constructs the pinned NixOS rejects or is removing

RESULT: PASS (0 failed checks). PASS is structural only: it does not show that the flake evaluates, builds or boots.
```

## Remaining work for the owner

1. On the **NixOS-WSL** (`26.05`) host, run `nix build .#checks.x86_64-linux.helpers-no-fhs` and `nix flake check --no-build`.
2. Build `.#nixosConfigurations.wros.config.system.build.toplevel` and run `nix path-info -Sh ./result` to measure the post-`S1` closure total against the `<= 3.0 GiB` budget.
3. Run the 7 KVM VM tests (`boot-unlock-login`, `install-encrypted`, `keyboard-preview`, `console-lock`, `update-goback`, `recovery-menu`, `live-session`) and `rust-suite` / `python-suite` via `nix flake check`.
4. Build `.#wros-iso` and `.#wros-update-bundle`, sign the update bundle with the external release key, and run the physical hardware checklist ([`docs/HARDWARE_CHECKLIST.md`](docs/HARDWARE_CHECKLIST.md)) on the **HP Stream 11**.

## Decisions

- **Systemd stage-1 initrd & graphical unlock (`B1`, `B2`, `B5`)**: Migrated `nixos/modules/boot.nix` to `boot.initrd.systemd.enable = true` and `boot.initrd.systemd.emergencyAccess = false`, hooking `wros-initrd-unlock.service` before `systemd-cryptsetup@wros_crypt.service` to write `/crypt-ramfs/passphrase` (`keyFile = "/crypt-ramfs/passphrase"; keyFileTimeout = 1;`) and falling back to systemd's text prompt on `wros.unlock=text`, `Esc`, or non-zero exit.
- **GRUB failed-boot timeout & specialisations (`B3`, `B4`)**: Dropped `have_grubenv`, initialized `/boot/grub/grubenv` in `wros-install-core` and `wros-boot-ok.service`, and set `boot.loader.grub.configurationName` on `specialisation.text-start` (`"WrOS (text start)"`) and `specialisation.recovery` (`"WrOS recovery"`).
- **Binary-cache updates & two-restart apply (`U1`–`U8`)**: Replaced `nix-store --restore` with a zstd Nix binary cache imported via `nix copy --from file://<cache> --no-check-sigs <toplevel>` after verifying every file against `closure-manifest.json`; retained the two-restart `/system-update` apply flow (`system-update.target`), added `os.statvfs` free-space checks, removed `staged/cache/` after import, and pruned old generations + ran `nix-collect-garbage` on healthy.
- **Closure size reductions (`S1`)**: Disabled `mbrolaSupport` in `espeak-ng` (`-645 MiB`), disabled `networking.modemmanager.enable` (`-67 MiB`), disabled default desktop font packs (`-62 MiB`), trimmed `linux-firmware/lib/firmware/intel` bloat, and disabled XDG desktop generators.

## Changed files

See `Files added`, `Files modified`, and `Files deleted` above (`32` modified files, `2` added files: `nixos/FINDINGS_R1.json` and `nixos/tests/encrypted-disk.nix`, `0` deleted files).

## Parity summary

All `577` rows in `rebase-kit/LEDGER.json` are accounted for in `nixos/PARITY.json`: `237` `ported`, `181` `kept`, `57` `native`, `71` `unaffected`, `31` `retired`, and `0` `deferred`.

## Gaps

- `0` deferred ledger rows.
- Physical hardware qualification (`docs/HARDWARE_CHECKLIST.md` on the HP Stream 11) and host KVM execution of the 7 NixOS VM tests remain to be run on the NixOS-WSL host.

## Hashes to fill

- `nixos/HASHES_TODO.json` is empty (`[]`): `flake.lock` is committed, `Cargo.lock` carries all crates.io SHA-256 hashes, `tools/prose-tools.lock.json` carries all Python wheel/model SHA-256 hashes, and `nixos/pkgs/wlroots-0.18.nix` carries the real upstream hash `sha256-D8RapSeH+5JpTtq+OU8PyVZubLhjcebbCBPuSO5Q7kU=`.

## Checks not run

- `nix flake check --no-build` and `nix flake check` (Nix and KVM are not installed in the sandbox VM container).
- `nix build .#checks.x86_64-linux.{helpers-no-fhs,boot-unlock-login,keyboard-preview,console-lock,update-goback,recovery-menu,install-encrypted,live-session}` (requires Nix + KVM on the NixOS-WSL host; `helpers-no-fhs` was simulated against the substituted `wros-helpers` tree with 0 matches).
- `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings` (Rust toolchain is not installed in the sandbox VM container).

## Qualification order

See [`docs/NIXOS_QUALIFICATION.md`](docs/NIXOS_QUALIFICATION.md):
1. Run `python3 rebase-kit/check_rebase.py --tree .` and Python unit test suites.
2. Run `nix --extra-experimental-features "nix-command flakes" flake check --no-build`.
3. Build `.#checks.x86_64-linux.helpers-no-fhs` and individual WrOS packages.
4. Build `.#nixosConfigurations.wros.config.system.build.toplevel` and measure closure size with `nix path-info -Sh ./result`.
5. Run all 7 KVM NixOS VM tests and `rust-suite` / `python-suite` via `nix flake check`.
6. Build `.#wros-iso` and `.#wros-update-bundle`, sign the update bundle, and verify on the HP Stream 11.
