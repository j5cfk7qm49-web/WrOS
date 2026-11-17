# WrOS installer design

The live dashboard and **Settings › System › Storage** can start the installer.
WrOS’s Rust installer uses the shared theme and keyboard navigation. A supervised
Python backend speaks its cdebconf-compatible protocol. The NixOS install core
partitions and encrypts the disk, copies the pre-built system with `nixos-install`,
creates the writer account, and installs GRUB last.
[Live session](LIVE_SESSION.md) · [Installation guide](INSTALLATION.md)

## Media and the target boundary

The NixOS ISO contains the complete pre-built installed-system closure in its
Nix store. Installation copies that closure offline; it does not use the Debian
backend, package pool or capsule. Build it with `nix build .#wros-iso`.
[Live image module](../nixos/modules/live.nix) · [Install core](../nixos/scripts/wros-install-core)

The supervisor requires exactly one unmounted eligible disk and records its
identity. The erase question names `/dev/<disk>`, warns that all its data will
be replaced, and starts at No.

The live account belongs to the live NixOS configuration; its greetd override is
generated under `/run/wros-live`. Installation copies a separate installed-system
closure without that account. Choosing a layout attempts to change the installer
keyboard immediately; if loading it fails, installation continues with the old
layout. The install core writes the selected layout to the target and ESP for
the first unlock. [Live account boundary](LIVE_SESSION.md)

## Input, credentials and failures

The writing reader retires before the installer receives the terminal. The
unprivileged frontend communicates with the backend through a private stream.
Password fields use a fixed hidden placeholder and bounded storage. Back clears
an unfinished answer. A cdebconf transport failure stops the backend rather than
approving an unanswered question. The NixOS backend implements the framing and
answer protocol in [wros-installer-backend](../nixos/scripts/wros-installer-backend).
The patched Debian module is retained only for Debian builds, rc13 and earlier.
[Input ownership](INPUT_OWNERSHIP.md)

## Qualification

The flake checks cover separate parts of the current installer:

- [install-encrypted](../nixos/tests/install-encrypted.nix) installs to an encrypted
  target, checks its labels and account boundary, then reboots from that disk.
- [live-session](../nixos/tests/live-session.nix) checks frontend handoff and
  cancellation without writing the target.
- [boot-unlock-login](../nixos/tests/boot-unlock-login.nix) checks disk unlock and
  authenticated login to the installed writing session.

Run each with `nix build .#checks.x86_64-linux.<check-name>`.
The [Debian installer VM harness](../os/tests/native_installer_vm.py) is historical,
for rc13 and earlier. Physical acceptance is recorded separately.
[Release checks](RELEASE_CHECKLIST.md) · [Hardware checks](HARDWARE_CHECKLIST.md)

Earlier builds and fault records are preserved in the archive. They include the
separate text media, matching-kernel backend, target review, keyboard adoption,
and encrypted-installation development runs. [Historical installer notes](archive/INSTALLER_MEDIA.md) · [Encrypted-installation record](archive/NATIVE_ENCRYPTED_INSTALL.md)
