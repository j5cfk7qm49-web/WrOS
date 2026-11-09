# Installing and building WrOS

## Install on an intended target

WrOS uses Debian 13 on UEFI amd64. The release-candidate media expects Secure
Boot to be disabled. Start with the checksum supplied with the candidate and a
verified backup of the target. The installer asks you to review the target
model, serial and size before you confirm disk changes. [Release checklist](RELEASE_CHECKLIST.md)

1. Boot the live medium and choose **Start WrOS**. The live dashboard offers
   **Install WrOS to disk**. Save live-session work to persistent storage before
   restarting. [Live session](LIVE_SESSION.md)
2. Open the installer and review the target and encryption choices. Debian’s
   maintained installer components do the storage, account and bootloader work.
   The disk password unlocks the encrypted system. The local account password
   signs into the writing session. [Installer design](NATIVE_INSTALLER.md)
3. When installation finishes, restart from the installed disk. Check that it
   unlocks, that you can sign in, and that your keyboard layout works at both
   prompts. Check backup restoration and recovery before relying on the machine
   for the only copy of a book. [Hardware checks](HARDWARE_CHECKLIST.md)

## Update an installed system

Open **Settings › Updates and recovery › Updates**. Review the version and
signer, then stage the signed package for the next restart. An app-only update
may offer to restart just the editor. The recovery page can repair an interrupted
update. **Go back** reinstalls the previous WrOS package when it is available.
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
The workflow lists the Debian build dependencies, including the native display
and input libraries. [Debian build prerequisites](../.github/workflows/ci.yml) · [Graphical design](GRAPHICAL_SESSION.md)

## Build media and qualify it in a VM

Use the isolated Debian 13 amd64 builder. The ordered live ISO recipe prints
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
