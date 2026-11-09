# WrOS installer design

The live dashboard and **Settings › System › Storage** can start the installer.
WrOS’s installer interface draws Debian cdebconf forms with the shared theme
and keyboard navigation. A separate supervisor manages the backend, private
mounts and terminal handoff. Debian’s maintained components perform the actual
partitioning, encryption, account setup, filesystem copy and bootloader work.
[Live session](LIVE_SESSION.md) · [Installation guide](INSTALLATION.md)

## Media and the target boundary

The live and installer kernels must match. The ISO recipe assembles the backend
and includes a regular-file capsule with the checked offline package pool.
The pool builder verifies signed Debian indexes and archive bytes. The capsule
stager rechecks the pool, matching kernel, and installed passthrough module.
[ISO recipe](../os/scripts/build-release-iso.sh) · [Pool builder](../tools/build-live-installer-pool.py) · [Capsule stager](../tools/stage-native-installer.py)

The target review presents the disk identity, partition information, and the
intended encrypted install. Destructive confirmations start at No. The live
account and its greetd override exist in the running overlay. The installer
copies the immutable filesystem and sets up the authenticated installed account.
The first installed boot adopts a supported layout selected during installation
and retains a layout already committed on the target.
[Live account boundary](LIVE_SESSION.md) · [Keyboard adoption evidence](evidence/installer-keyboard-vm.json)

## Input, credentials and failures

The writing reader retires before the installer receives the terminal. The
unprivileged frontend communicates with the backend through a private stream.
Password fields use a fixed hidden placeholder and bounded storage. Back clears
an unfinished answer. A cdebconf transport failure stops the backend rather than
approving an unanswered question. The retained Debian patch covers framing and
answer boundaries. [Input ownership](INPUT_OWNERSHIP.md) · [cdebconf patch](../os/config/installer/README.md)

## Qualification

Use a disposable marked target to test review and cancellation, encrypted-root
ancestry, retirement of the installer's target mounts, and an authenticated
reboot. The release chain then starts from the saved installed disk and checks
the update and startup paths. Physical acceptance is recorded separately.
[Installer VM harness](../os/tests/native_installer_vm.py) · [Release checks](RELEASE_CHECKLIST.md) · [Hardware checks](HARDWARE_CHECKLIST.md)

Earlier builds and fault records are preserved in the archive. They include the
separate text media, matching-kernel backend, target review, keyboard adoption,
and encrypted-installation development runs. [Historical installer notes](archive/INSTALLER_MEDIA.md) · [Encrypted-installation record](archive/NATIVE_ENCRYPTED_INSTALL.md)
