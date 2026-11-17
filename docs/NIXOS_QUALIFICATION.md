# WrOS NixOS qualification guide

This guide qualifies the NixOS release in [os/VERSION](../os/VERSION) on a NixOS-WSL host with KVM, followed by physical hardware verification.

## Step 1: Run the in-tree checks & Python unit tests

Before invoking Nix, run the in-tree documentation checker and Python unit tests:

```sh
python3 tools/check-wros.py
python3 os/tests/test_nixos_update.py -v
python3 os/tests/test_backup.py -v
python3 os/tests/test_keyboard.py -v
python3 os/tests/test_boot_settings.py -v
python3 os/tests/test_name_leftovers.py -v
python3 os/tests/test_line_endings.py -v
```

The maintained Nix Python suite is `nix build .#checks.x86_64-linux.python-suite`;
its test list is defined in [nixos/pkgs/default.nix](../nixos/pkgs/default.nix).

## Step 2: Verify `flake.lock` and evaluate all flake outputs

`flake.lock` is committed and pins `nixpkgs` (`f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0`) and `rust-overlay` (`84edd48f141b0c8910dffdd0296ddd577e829132`), and `nixos/pkgs/wlroots-0.18.nix` pins `wlroots` `0.18.3` (`sha256-D8RapSeH+5JpTtq+OU8PyVZubLhjcebbCBPuSO5Q7kU=`). Evaluate all flake outputs without building:

```sh
nix --extra-experimental-features "nix-command flakes" flake check --no-build
```

## Step 3: Build individual WrOS packages & run `helpers-no-fhs`

Build the Rust binaries, C helpers, patched Cage compositor, compiled console keymaps, prose runtime, and helper scripts, and run the `helpers-no-fhs` gate (`H1`):

```sh
nix build .#grimoire .#wros-greeter .#wros-boot .#wros-installer
nix build .#wros-console-reset .#wros-display-hw .#wros-cage .#wros-keymaps .#wros-helpers .#wros-licenses
nix build .#checks.x86_64-linux.helpers-no-fhs
```

## Step 4: Build the installed system closure & measure the `S1` closure reduction

Build `nixosConfigurations.wros.config.system.build.toplevel` and inspect its closure size against the `<= 3.0 GiB` budget (`S1`):

```sh
nix build .#nixosConfigurations.wros.config.system.build.toplevel
nix path-info -Sh ./result
nix path-info -rS ./result | sort -nk2 | tail -25
```

Compare the measured closure total and largest store paths against the Round 2 baseline (`4.1 GiB` across `918` paths) and the individual removals documented in [`docs/NIXOS_BASE.md`](NIXOS_BASE.md).

## Step 5: Run the Rust, Python, and eight KVM NixOS VM suites

On NixOS-WSL with `/dev/kvm` accessible:

```sh
nix build .#checks.x86_64-linux.rust-suite
nix build .#checks.x86_64-linux.python-suite
nix build .#checks.x86_64-linux.install-encrypted
nix build .#checks.x86_64-linux.boot-unlock-login
nix build .#checks.x86_64-linux.keyboard-preview
nix build .#checks.x86_64-linux.console-lock
nix build .#checks.x86_64-linux.update-goback
nix build .#checks.x86_64-linux.recovery-menu
nix build .#checks.x86_64-linux.live-session
nix build .#checks.x86_64-linux.live-graphical
nix flake check
```

## Step 6: Build the offline live ISO and binary-cache update bundle

Build the self-contained installation ISO (`wros-iso`) and the unsigned binary-cache update bundle (`wros-update-bundle`), then sign the bundle with the external release key:

```sh
nix build --out-link result-iso .#wros-iso
nix build --out-link result-update-bundle .#wros-update-bundle
python3 -I tools/build-nixos-update-bundle.py ./result-update-bundle \
  --out ./out/nixos-updates \
  --gnupg-home "$EXTERNAL_GNUPGHOME" \
  --key-id B9F5B97C9370239CC07B425F2CB426FC6D27CA65 \
  --expect-fingerprint B9F5B97C9370239CC07B425F2CB426FC6D27CA65
```

## Step 7: Physical hardware verification on the HP Stream 11

Flash `result-iso/iso/*.iso` to a USB drive, boot the HP Stream 11 (`Celeron N4000`, `4 GB RAM`, `32 GB eMMC`, `Intel UHD 600`, `RTL8822BE Wi-Fi`) with UEFI and Secure Boot disabled, install to the internal eMMC via **Install WrOS**, and complete every item in [`docs/HARDWARE_CHECKLIST.md`](HARDWARE_CHECKLIST.md). Only after physical verification passes may `docs/feature-status.json` entries be marked `Hardware verified`.
