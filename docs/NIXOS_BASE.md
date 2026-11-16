# WrOS NixOS base decision record (`0.1.0~rc14`)

This document records the architecture, disk layout, bootloader and systemd stage-1 initrd choices, mutable-state boundary, closure size budget, and firmware selection for the WrOS `0.1.0~rc14` NixOS operating-system layer defined in [`flake.nix`](../flake.nix) and [`nixos/modules/`](../nixos/modules).

## 1. Appliance model & evaluation boundary

- **Pinned inputs**: `nixpkgs` branch `nixos-26.05` at revision `f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0` and `rust-overlay` at revision `84edd48f141b0c8910dffdd0296ddd577e829132` (Rust `1.99.1`), targeting `x86_64-linux` UEFI only (Secure Boot off).
- **Zero on-device Nix evaluation or compilation**: The installed appliance never runs `nixos-rebuild`, `nix-build`, `nix build`, `nix-channel`, `nix-shell`, `nix develop`, `nix flake`, or `nixos-generate-config`, and never parses `.nix` files at runtime.
  - `nix.channel.enable = false` (`nixos/modules/base.nix`).
  - `nix.nixPath = lib.mkForce []` (`nixos/modules/base.nix`).
  - `nix.registry = lib.mkForce {}` (`nixos/modules/base.nix`).
  - `nix.settings.experimental-features = []` and `nix.settings.substituters = lib.mkForce []` (`nixos/modules/base.nix`).
  - `system.disableInstallerTools = true` (`nixos/modules/base.nix`).
  - No C/Rust compiler or build tool (`gcc`, `clang`, `rustc`, `cargo`, `gnumake`, `cmake`, `meson`, `ninja`) is present in `environment.systemPackages` (`nixos/modules/base.nix`).
- **Per-derivation source filtering (`U7`)**: Every derivation in [`nixos/pkgs/`](../nixos/pkgs) uses `lib.fileset.toSource` restricted to the files that derivation actually reads (`rust-packages.nix`, `c-helpers.nix`, `cage.nix`, `keymaps.nix`, `prose-runtime.nix`, `licenses.nix`, `helpers.nix`), and each Rust binary receives only the `WROS_PATH_*` environment variables it reads (`bootPathEnv`, `installerPathEnv`, `greeterPathEnv`, `rustPathEnv` in `nixos/pkgs/paths.nix`, `H6`). Editing a document, module, or helper script does not invalidate the Rust or C package store paths.

## 2. Partitioning & filesystem labels

All partitions and encrypted containers are mounted strictly by filesystem or partition label so one pre-built system closure boots on any target laptop without per-machine `hardware-configuration.nix` edits or literal disk UUIDs:

| Role | Label / Mapper | Filesystem | Mountpoint | Declared in |
|---|---|---|---|---|
| EFI System Partition (512 MiB) | `WROS-ESP` (`/dev/disk/by-label/WROS-ESP`) | `vfat` (`fmask=0077,dmask=0077`) | `/boot` | [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) |
| LUKS2 encrypted partition | `WROS-LUKS` (`/dev/disk/by-label/WROS-LUKS`) | `crypto_LUKS` (Argon2id) | `/dev/mapper/wros_crypt` | [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) |
| Encrypted root filesystem | `WROS-ROOT` (`/dev/disk/by-label/WROS-ROOT`) | `ext4` (`noatime`) | `/` | [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) |
| Live installation ISO | `WROS_LIVE` | `iso9660` + `/iso/nix-store.squashfs` | `/` (live) | [`nixos/modules/live.nix`](../nixos/modules/live.nix) |
| Offline update USB media | `WROS-UPDATE` | `ext4`, `vfat`, or `iso9660` | `/run/wros-update/media/<n>` | [`nixos/scripts/wros-update-engine.py`](../nixos/scripts/wros-update-engine.py) |

## 3. Bootloader, boot-check timeout & recovery entries (`B3`, `B4`)

- **GRUB UEFI bootloader**: `boot.loader.grub.enable = true`, `efiSupport = true`, `efiInstallAsRemovable = true`, `boot.loader.efi.canTouchEfiVariables = false`, `device = "nodev"`, and `configurationLimit = 3` (`nixos/modules/boot.nix`).
- **2-second hidden menu / 30-second failed-boot menu (`B3`)**: NixOS's `install-grub.pl` automatically emits `if [ -s $prefix/grubenv ]; then load_env; fi` at the top of `/boot/grub/grub.cfg` without setting any `have_grubenv` variable. `nixos/scripts/wros-install-core` creates the 1024-byte `/boot/grub/grubenv` block at install time, and `boot.loader.grub.extraConfig` in [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) checks `if [ "${wros_boot_pending}" = "1" ]` directly: if the previous boot did not reach the login screen, GRUB sets `timeout_style=menu` and `timeout=30`; otherwise it uses `timeout_style=hidden` with `timeout=2`, then sets `wros_boot_pending=1` and runs `save_env wros_boot_pending`.
- **Clearing the pending boot mark**: `systemd.services.wros-boot-ok` (`nixos/modules/boot.nix`), ordered `after = [ "greetd.service" ]`, ensures `/boot/grub/grubenv` exists (`grub-editenv /boot/grub/grubenv create` if missing or empty) and runs `grub-editenv /boot/grub/grubenv unset wros_boot_pending` once the login screen is reached.
- **Boot entries (`B4`)**: Specialisations in [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) provide the two additional boot menu entries using `boot.loader.grub.configurationName` (which `install-grub.pl` reads from `$link/configuration-name`):
  - `specialisation.text-start`: sets `boot.loader.grub.configurationName = "WrOS (text start)"` and `boot.kernelParams = [ "wros.unlock=text" "wros.session=console" ]`.
  - `specialisation.recovery`: sets `boot.loader.grub.configurationName = "WrOS recovery"` and `boot.kernelParams = [ "wros.unlock=text" "systemd.unit=wros-recovery.target" ]`.

## 4. Systemd stage-1 initrd & graphical disk unlock without rebuilds (`B1`, `B2`, `B5`)

- **Systemd stage-1 initrd (`B2`, `B5`)**: [`nixos/modules/boot.nix`](../nixos/modules/boot.nix) sets `boot.initrd.systemd.enable = true` and `boot.initrd.systemd.emergencyAccess = false` (so a failed unlock or root mount never drops to an initrd emergency shell). `wros-boot`, `wros-console-reset`, `loadkeys`, and the compiled console keymaps (`wros-keymaps` at `/etc/wros/keymaps`) are included in the initrd via `boot.initrd.systemd.extraBin`, `boot.initrd.systemd.storePaths`, and `boot.initrd.systemd.contents`.
- **Graphical unlock hooked into `systemd-cryptsetup@wros_crypt.service` (`B1`)**: Following the Clevis ramfs key pattern in `nixpkgs/nixos/modules/system/boot/luksroot.nix` (lines 1251–1289), `boot.initrd.luks.devices."wros_crypt"` sets `keyFile = "/crypt-ramfs/passphrase"` and `keyFileTimeout = 1` (with `fallbackToPassword` implied by systemd stage 1), and `boot.initrd.systemd.services.wros-initrd-unlock` runs `before = [ "systemd-cryptsetup@wros_crypt.service" ]`:
  1. Mounts a `0700` `ramfs` at `/crypt-ramfs` (unmounted by `ExecStop`).
  2. Mounts `/dev/disk/by-label/WROS-ESP` read-only (`ro,nodev,nosuid,noexec`) and copies `keyboard.json`, `boot-theme.toml`, and `boot-screen` from `/boot/wros-boot/` to `/etc/wros/` with a strict `head -c 8192` size cap, refusing symlinks.
  3. Extracts the `"layout"` field from `/etc/wros/keyboard.json` and allows only the four compiled layout names (`qwerty`, `colemak`, `colemak-dh-ansi`, `colemak-dh-iso`) before calling `/bin/loadkeys --unicode /etc/wros/keymaps/$layout.map`; no path from `/boot/wros-boot/` is ever executed.
  4. Runs `/bin/wros-boot unlock --tty /dev/console --theme /etc/wros/boot-theme.toml` to write `/crypt-ramfs/passphrase`.
- **How fallback to the standard text prompt is reached (`B1`)**: Whenever `wros.unlock=text` is present on `/proc/cmdline`, `/etc/wros/boot-screen` is not `ratatui`, the writer presses `Esc` in `wros-boot unlock`, or `wros-boot unlock` exits non-zero (or if the typed passphrase does not unlock LUKS), `/crypt-ramfs/passphrase` is absent (or rejected after `keyFileTimeout = 1`), `wros-console-reset` resets the console, and `systemd-cryptsetup@wros_crypt.service` automatically prompts on the console with systemd's standard text passphrase prompt (using the already-loaded console keymap).

## 5. Mutable state table (`D1`, `H3`)

NixOS regenerates files declared in `environment.etc` on every activation. Only immutable release identity, version (`environment.etc."wros/version"`), update trust anchors (`environment.etc."wros/update-trust/*"`), `/etc/greetd/config.toml`, `/etc/pam.d/*`, `/etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf`, and `/etc/wros/defaults/keyboard.json` are declared in `environment.etc`. All writer-mutable settings live outside `environment.etc` and are seeded by `systemd.tmpfiles.rules` in [`nixos/modules/base.nix`](../nixos/modules/base.nix) or by [`nixos/scripts/wros-install-core`](../nixos/scripts/wros-install-core):

| Setting / State | Mutable path(s) | Seeded / written by | In `environment.etc`? |
|---|---|---|---|
| Keyboard layout | `/etc/wros/keyboard.json` and `/boot/wros-boot/keyboard.json` | Seeded by `tmpfiles.d` (`C /etc/wros/keyboard.json ... /etc/wros/defaults/keyboard.json`) and `wros-install-core`; updated by `wros-keyboard` | **No** (only `/etc/wros/defaults/keyboard.json` is in `environment.etc`) |
| Display settings | `/etc/wros/display.json` and `~/.config/wros/session.json` | Written by `wros-display` and session settings when configured | **No** |
| Boot theme | `/etc/wros/boot-theme.toml` and `/boot/wros-boot/boot-theme.toml` | Written by `wros-boot-settings` when the writer changes the start-screen theme; absent until configured (`wros-boot` uses its built-in default theme when absent) | **No** |
| Boot-screen switch | `/etc/wros/boot-screen` and `/boot/wros-boot/boot-screen` | Written by `wros-boot-settings` (`ratatui` or `text`); defaults to `ratatui` when absent | **No** |
| XKB layout | `/etc/default/keyboard` | Seeded by `tmpfiles.d` (`f /etc/default/keyboard ...`) and `wros-install-core`; updated by `wros-keyboard` | **No** |
| Timezone | `/etc/localtime` and `/etc/timezone` | `time.timeZone = null`; seeded by `wros-install-core`; updated by `timedatectl` / `wros-clock` | **No** |
| Hostname | `/etc/hostname` | `networking.hostName = lib.mkDefault ""` in `nixos/modules/base.nix` so NixOS does not overwrite `/etc/hostname`; seeded by `tmpfiles.d` (`f /etc/hostname ... wros\n`) and `wros-install-core` (`H3`) | **No** |
| Accounts & passwords | `/etc/passwd`, `/etc/shadow`, `/etc/group` | `users.mutableUsers = true`; writer account created by `wros-install-core`, updated by `wros-passwd` | **No** |
| Wi-Fi profiles | `/etc/NetworkManager/system-connections/` | Created `0700` by `tmpfiles.d` (`d /etc/NetworkManager/system-connections 0700 root root -`) and `wros-install-core`; written by NetworkManager (`nmcli`) | **No** |
| Update state & journal | `/var/lib/wros-update/` and `/nix/var/nix/gcroots/wros/` | Created by `tmpfiles.d` and managed by `wros-update-engine.py` | **No** |

## 6. Closure size baseline & Round 2 size reductions (`S1`, `D1`)

### Measured Round 2 baseline closure (before `S1` changes)

At the Round 2 baseline (`e7319a1`), `nix path-info -S` on the built `nixosConfigurations.wros` closure measured **`4.1 GiB` across `918` store paths** against the **`<= 3.0 GiB`** budget (`1.1 GiB` over budget):

| Baseline component | Measured size at `e7319a1` | Notes |
|---|---|---|
| `mbrola-voices` | `645 MiB` | Pulled in by default `pkgs.espeak-ng` (`mbrolaSupport = true` in nixpkgs); Debian `rc13` never shipped MBROLA diphone voice databases |
| `llvm-lib` | `540 MiB` | Pulled in by `pkgs.mesa` (shader compiler backend for DRI/Vulkan drivers) |
| `wros-firmware` | `355 MiB` | Included the entire `linux-firmware/lib/firmware/intel` directory alongside `i915` and Wi-Fi directories |
| `mesa` | `264 MiB` | OpenGL/EGL/GBM/Vulkan drivers used by `wros-cage` and `grimoire` |
| `wros-prose-runtime` | `155 MiB` | Python virtualenv with `spacy`, `en_core_web_sm`, `proselint`, `nabokov`, `numpy` |
| Linux kernel modules | `144 MiB` | `pkgs.linuxPackages` kernel modules |
| `python3` | `128 MiB` | System Python 3 runtime used by helpers and prose runtime |
| `rclone` | `109 MiB` | Used by `wros-backup` and `wros-sync` for Google Drive |
| `libqmi` (+ `ModemManager`) | `67 MiB` | Cellular modem support enabled by default by `networking.networkmanager.enable = true` |
| `noto-fonts-cjk-sans` | `62 MiB` | Pulled in by NixOS's default `fonts.enableDefaultPackages` |
| `flite` | `60 MiB` | Transitive dependency in default multimedia packages |
| Linux kernel (`bzImage`) | `13.0 MiB` | Copied to `/boot/kernels/` (`WROS-ESP`) |
| Stage-1 `initrd` | `19.5 MiB` | Copied to `/boot/kernels/` (`WROS-ESP`) |

*(Note: LibreOffice and Pandoc are not in the WrOS closure; `grimoire` exports DOCX and EPUB natively in Rust.)*

### Round 2 closure removals and individual expected savings (`S1`)

Because Nix cannot be executed inside the authoring container, the new post-trim total cannot be measured here and no new total is claimed. Each removal applied in Round 2, its individual expected saving from the measured baseline above, and its feature impact are listed below for measurement on the NixOS-WSL host:

1. **Disable MBROLA voices in `espeak-ng` (`(pkgs.espeak-ng.override { mbrolaSupport = false; })` in `nixos/modules/base.nix` and `nixos/modules/audio.nix`)**:
   - **Expected saving**: **`645 MiB`** (`mbrola-voices` plus the `mbrola` binary).
   - **Feature impact**: None for WrOS parity — `espeak-ng`'s built-in formant voices remain installed for read-aloud (`src/voice.rs`), matching Debian `rc13`, which never installed `mbrola` or `mbrola-*` voices. Only optional third-party `mb-*` MBROLA diphone voices are omitted.
2. **Disable cellular ModemManager (`networking.modemmanager.enable = false;` in `nixos/modules/network.nix`)**:
   - **Expected saving**: **`67 MiB`** (`libqmi` plus `ModemManager` and `libmbim`).
   - **Feature impact**: Cellular WWAN/LTE modems are not managed by NetworkManager; Wi-Fi (`wpa_supplicant`, `nmcli`) and Ethernet are unaffected.
3. **Disable default desktop font packs (`fonts.enableDefaultPackages = false; fonts.packages = [ pkgs.dejavu_fonts ];` in `nixos/modules/base.nix`)**:
   - **Expected saving**: **`62 MiB`** (`noto-fonts-cjk-sans` and default desktop font packs).
   - **Feature impact**: The console session uses its embedded Unifont glyph atlas (`crates/wros-graphics`), and the graphical Wayland session (`foot` / `grimoire` under `wros-cage`) uses `DejaVu Sans Mono`. Only non-Latin CJK fallback fonts in external Wayland apps are omitted.
4. **Remove `linux-firmware/lib/firmware/intel` from `wrosFirmware` (`nixos/modules/hardware.nix`)**:
   - **Expected saving**: Removes the `intel/` subdirectory (Intel Bluetooth `ibt-*`, IPU6 camera, Ice Lake/Xeon server NIC, and DSP audio blobs) from the `355 MiB` `wros-firmware` package while retaining `i915/` (Intel UHD 600 GuC/HuC/DMC display firmware), `iwlwifi*`, `rtlwifi`, `rtw88` (Realtek RTL8822BE on the HP Stream 11), `rtw89`, `ath9k_htc`, `ath10k`, `ath11k`, `brcm`, `mediatek`, `wireless-regdb`, and `sof-firmware`.
   - **Feature impact**: Intel Bluetooth and Intel IPU6 webcams lack firmware blobs; Wi-Fi (`iwlwifi`, Realtek, Atheros, Broadcom, MediaTek), Intel `i915` graphics, and SOF audio are unaffected.
5. **Disable XDG desktop sound/icon/menu/mime/autostart generators (`xdg.{sounds,icons,menus,mime,autostart}.enable = false;` in `nixos/modules/base.nix`) and drop the unused `pythonWithQr` (`qrcode` + `pillow`) wrapper in `nixos/pkgs/helpers.nix`**:
   - **Expected saving**: Removes `freedesktop` sound/icon themes and the duplicate `python3.withPackages (ps: [ ps.qrcode ps.pillow ])` environment (since `os/scripts/wros-link.py` renders QR codes in pure Python stdlib).
   - **Feature impact**: None; WrOS runs `grimoire` directly in `wros-cage` without a desktop application menu or XDG icon theme.

## 7. Retired Debian helpers on NixOS (`H1`)

- **`wros-name-migrate` (retired on NixOS)**: On Debian, `os/scripts/wros-name-migrate.py` migrated `0.1.0~rc3` installations from the WritingDeck name to `wros` in place. Every NixOS installation starts at `0.1.0~rc14+` with the `wros` group and its `writingdeck` compatibility group created declaratively in `nixos/modules/base.nix`, so `wros-name-migrate` has no purpose on NixOS and is not installed into `wros-helpers`.
- **`dpkg-query` in `wros-session` (`H1`)**: `os/scripts/wros-session` calls `dpkg-query` only inside ` debian_session_note()` as a fallback diagnostic string when `wros-cage` fails to start. On NixOS (`nixos/pkgs/helpers.nix`), `/usr/bin/dpkg-query` is replaced with `${pkgs.coreutils}/bin/false`, which `debian_session_note()` handles cleanly.
