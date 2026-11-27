# Installing and building WrOS

## Install on an intended target

From 0.1.0~rc14 WrOS is built on NixOS 26.05, for UEFI amd64 with Secure Boot disabled. Check
the ISO against the SHA-256 published with the release, and write it to a USB stick (it is a
hybrid image: write the whole file to the whole stick). Rufus's **DD image mode** or balenaEtcher
does that; from rc16 Rufus's default "ISO image mode" (a FAT32 copy of the files) also works.
Have a verified backup of anything on the target disk. [Release checklist](RELEASE_CHECKLIST.md)

1. Start the laptop from the stick and choose **Start WrOS** in its menu. The live dashboard
   offers **Install WrOS to disk**. Save live-session work to persistent storage before
   restarting. [Live session](LIVE_SESSION.md)
2. Choose the keyboard layout, a host name, your name, login name and password, and the disk
   password. The installer works without a network. Before it erases anything it checks again
   that the disk is the one it showed you and that nothing on it is in use. It makes the disk
   bootable only as its very last step, so an install that stops part-way leaves a disk that
   doesn't start rather than a broken one; run the installer again.
3. When installation finishes, restart from the installed disk. Check that it unlocks, that you
   can sign in, and that your keyboard layout works at both prompts. Check backup restoration
   and recovery before relying on the machine for the only copy of a project.
   [Hardware checks](HARDWARE_CHECKLIST.md)

## Moving from rc13 (Debian) to NixOS

rc13 can't update to NixOS in place: the whole system underneath changes. On rc13, make sure
**Settings › Backups and accounts › Backups** shows a recent backup of every project. Then install
the current NixOS release from the stick as above, set up the same backup destination (and
recovery key) on the current NixOS release, and restore each project from it. The Debian update channel is retired.

## Update an installed system

rc14 and rc15 cannot install updates from Settings: their app rejects the NixOS updater's
replies (#143, fixed in rc16). An rc15 laptop reaches rc16 or later with one update run from
a root shell through `sudo`; rc14 has no sudo (#142), so reinstall it from the current stick
and restore your projects from backup.

From rc16, open **Settings › Updates and recovery › Updates**. Review the version and signer,
then stage the signed update for the next restart: WrOS restarts once to install it as a new system
generation and again into it. **Go back** returns to the previous generation, which NixOS
keeps. The recovery page checks store integrity and repairs the boot generation.
[Rescue guide](RC_RESCUE.md)

## Run the editor in a development terminal

In a Debian 13 builder with the repository’s Rust toolchain and locked
dependencies, run as a normal user. Use a new path for the demo.

```sh
cargo run --locked --no-default-features -- wros tutorial-demo /path/to/new-demo
cargo run --locked --no-default-features -- /path/to/new-demo
cargo run --locked --no-default-features -- wros status --simulate
```

For the native graphical build, use `--no-default-features --features gui-drm`.
`nix develop` gives a shell with the pinned toolchain and the native display and input
libraries. [Development shell](../flake.nix) · [Graphical design](GRAPHICAL_SESSION.md)

## NixOS appliance build and installation (rc14 and later)

For the NixOS operating-system layer (`nixosConfigurations.wros` and `nixosConfigurations.wros-live`), the live ISO (`nix build .#wros-iso`) embeds the complete pre-built system closure in `/iso/nix-store.squashfs` and installs offline to `WROS-ESP` + `WROS-LUKS` (`wros_crypt` -> `WROS-ROOT`) via `wros-install-session`, `wros-installer-backend`, and `wros-install-core`. No Nix evaluation or compilation runs on the target laptop. See [`NIXOS_BASE.md`](NIXOS_BASE.md) and [`NIXOS_QUALIFICATION.md`](NIXOS_QUALIFICATION.md).
