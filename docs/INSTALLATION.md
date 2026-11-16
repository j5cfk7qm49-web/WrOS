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
   and recovery before relying on the machine for the only copy of a book.
   [Hardware checks](HARDWARE_CHECKLIST.md)

## Moving from rc13 (Debian) to rc14 (NixOS)

rc13 can't update to rc14 in place: the whole system underneath changes. On rc13, make sure
**Settings › Backups and accounts › Backups** shows a recent backup of every book. Then install
rc14 from the stick as above, set up the same backup destination (and recovery key) on rc14, and
restore each book from it. Your Debian update channel stays as it was until you reinstall.

## Update an installed system

Open **Settings › Updates and recovery › Updates**. Review the version and signer, then stage
the signed update for the next restart: WrOS restarts once to install it as a new system
generation and again into it. **Go back** returns to the previous generation, which NixOS
keeps. The recovery page can repair an interrupted update. Until an rc14 update is published,
the page says no WrOS updates have been published yet. [Rescue guide](RC_RESCUE.md)

## Run the editor in a development terminal

In a Debian 13 builder with the repository’s Rust toolchain and locked
dependencies, run as a normal user. Use a new path for the demo.

```sh
cargo run --locked --no-default-features -- wros tutorial-demo /path/to/new-demo
cargo run --locked --no-default-features -- /path/to/new-demo
cargo run --locked --no-default-features -- wros status --simulate
```

For the native graphical build, use `--no-default-features --features gui-drm`.
The workflow lists the Debian build dependencies, including the native display
and input libraries. [Debian build prerequisites](../.github/workflows/ci.yml) · [Graphical design](GRAPHICAL_SESSION.md)

## Build Debian media (rc13 and earlier) and qualify it in a VM

For the Debian releases, use the isolated Debian 13 amd64 builder. The ordered live ISO recipe prints
its steps with the following command. It builds the matching installer backend,
assembles the offline package pool, audits the ISO and smoke-boots it before
placing it under the release name. The private update-signing key stays outside
that builder. [ISO build script](../os/scripts/build-release-iso.sh)

```sh
bash os/scripts/build-release-iso.sh --plan
```

The separate `bootstrap-rootfs.sh --vm-demo` and `assemble-image.sh` recipes
build a disposable UEFI test disk with an automatic demo login. Use the release
chain for an authenticated encrypted installation and package update. Test
results refer to the exact artifacts and the checks that ran.
[VM profile](../os/scripts/bootstrap-rootfs.sh) · [Qualification steps](TESTING.md)


## NixOS appliance build and installation (`0.1.0~rc14`)

For the NixOS operating-system layer (`nixosConfigurations.wros` and `nixosConfigurations.wros-live`), the live ISO (`nix build .#wros-iso`) embeds the complete pre-built system closure in `/iso/nix-store.squashfs` and installs offline to `WROS-ESP` + `WROS-LUKS` (`wros_crypt` -> `WROS-ROOT`) via `wros-install-session`, `wros-installer-backend`, and `wros-install-core`. No Nix evaluation or compilation runs on the target laptop. See [`NIXOS_BASE.md`](NIXOS_BASE.md) and [`NIXOS_QUALIFICATION.md`](NIXOS_QUALIFICATION.md).
