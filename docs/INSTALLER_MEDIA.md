# Experimental offline Debian Installer media

This continues the existing Debian 13 appliance. The separate VM demo remains
available; it is not an installation image. The new ISO uses Debian's maintained
`live-build` and `live-installer` to copy the complete writing filesystem, then
uses Debian Installer for storage, encryption, accounts and the bootloader.
See the [Debian Live installer documentation](https://live-team.pages.debian.net/live-manual/html/live-manual/customizing-installer.en.html)
and [Debian account/storage options](https://www.debian.org/releases/trixie/amd64/apbs04.en.html).

The current integrated recipe now ships the native WrOS tty1 greeter backed by
Debian greetd/PAM, the current editor/input reader, categorized Settings and
first-install keyboard adoption. An actual image audit requires the locked
non-root greeter account, fixed session, PAM/package notices, enabled service,
exclusive tty1 ownership, authenticated tty2 maintenance and sleep/power guards.
The older image records below keep their original source identities. Current
fresh-build and installed-boot qualification are in progress.

The initial ISO built successfully on 2026-10-26. Its actual media audit passed:
the squashfs and installer boot files exist, the six required Wi-Fi firmware
archives are in the media pool, and the installed filesystem has their real
firmware files, kernel drivers, connection tools and copyright notices. It has
no VM probe, demo account or autologin configuration, and root is locked before
the installer creates a normal account. This is payload evidence, not complete
installation or hardware qualification. Issue #3 remains open.

The first no-NIC install powered off normally after 1,016.21 seconds and its
disk reached a real [tty1 login prompt](evidence/installed-login-exploratory.png)
through OVMF, without the ISO or a directly supplied kernel. Its automated probe
remains failed: `in-target` routed payload markers to syslog, and the first boot
fixture had not propagated the serial console after Debian's `---` separator.
The corrected probe is being rerun. These observations are retained in
[exploratory evidence](evidence/installer-exploratory.json), without relabeling
the failed probe as passed or claiming that credentials were tested in that attempt.

The third fresh installation and subsequent auth-only retry now pass the bounded
[installed-image contracts](evidence/installer-vm.json): real UEFI boot, wrong
password rejection, authenticated UID-1000 maintenance login, refusal of
passwordless sudo, non-echoing tty1 PAM input, the actual non-root
[writing dashboard](evidence/installed-writing-session.png), and six installed
Wi-Fi firmware packages plus nmtui and kernel wireless modules. The test had
zero NICs and no host disks. Its 1,044.5-second measurement covers installation
only; the later authenticated boot/retry is separate. Earlier failures remain
retained. The generated test secret was removed after success.

That original ISO contains the earlier qualified M7b binary, whose exact hash is
recorded. It does not qualify later source-path/Git fixes. A separate clean native
Debian build now passed at `59cc34f`, including those fixes and the revised recipe:
[exact input, ISO, signature-chain and payload identities](evidence/installer-clean-build.json).
The source and all measured build inputs were unchanged across that build. Its
ISO SHA-256 is `6c3a66746b9bc94bff0b7917f79aa4a3aea71922c821ae3ab0568ec875fa4a56`.
Its full install test reached the actual payload/late-command markers but hit
the 1,200-second fixture deadline during finalization. That failure is retained.
A separate [post-interruption boot probe](evidence/installer-current-interrupted.json)
passed real UEFI/PAM/password-required sudo/non-root UI and installed Wi-Fi
contracts, with an [actual current-image screenshot](evidence/installed-current-session.png).
It does not turn the failed normal-install record into a pass. This image also
passed [actual no-disk refusal and cancellation](evidence/installer-safety.json):
finish an empty plan and observe refusal; select Finish interactively, answer No,
require a newly received partition menu and an unchanged whole-target SHA-256.
No NIC or host disk was exposed.

## Build

Use an isolated Debian 13 amd64 builder on a Linux filesystem with enough room
for chroot/cache/ISO and disposable test disks. Install `live-build debootstrap
squashfs-tools xorriso isolinux syslinux-common grub-pc-bin grub-efi-amd64-bin
cpio file zstd rsync gpgv debian-archive-keyring` plus the appliance build
prerequisites in [INSTALLATION](INSTALLATION.md). Build the existing Grimoire
workspace binaries and exact Python runtime first, as described in [M7B](M7B.md).

```sh
cargo build --workspace --locked --no-default-features
sudo env GRIMOIRE_BINARY=/absolute/path/to/grimoire \
  WROS_GREETER_BINARY=/absolute/path/to/wros-greeter \
  WRITINGDECK_PROSE_RUNTIME=/absolute/path/to/runtime \
  bash os/scripts/build-installer.sh
```

The builder accepts no target-disk argument and preserves any existing
`build/installer` contents by refusing to start over them. It writes ordinary
build artifacts and performs chroot operations; it does not install on the host.
The output is `build/installer/wros-installer-amd64.hybrid.iso` plus hashes,
resolved package files and audits. Build downloads need connectivity; installation
must be tested with no network. APT signature checks stay enabled. The separate
installer-image verifier checks the kernel/initrd through Debian's archive
keyring, signed InRelease and authenticated SHA256SUMS before live-build unpacks
them. This does not provide a WrOS release signature or update/rollback policy.
The first exploratory build verified that same chain after assembly and before
boot; the recipe now moves verification earlier and checks all prerequisites.
The clean end-to-end rerun of that revised recipe passed; its current-image
installation and safety acceptance remain separate gates.

The shipping preseed only enables live copying, requires a normal account,
disables direct root login and selects ordinary account groups including
`writingdeck`. **It contains no disk, password, partition recipe or erase
confirmation.** Debian asks for language, keyboard, timezone and account
credentials. Its disk/encryption review and write-confirmation screens remain in control. Wi-Fi
firmware is present both as installer archives and in the installed filesystem;
an initial network download is not required for that payload.

## Acceptance and limits

Five non-root Debian regression tests check actual firmware archive metadata,
missing-media refusal, autologin/demo/root-account rejection, the interactive
preseed boundary, hash tampering and actual unsigned-release rejection by gpgv.
ShellCheck and Python syntax checks pass. Actual Debian/macOS CI
[`40448965974`](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40448965974)
passed on `bf22273`: 912 Rust tests per overlapping Debian configuration, both
Clippys, 75 Python methods with zero skips and all 17 software gates. Its
[software record](evidence/ci-40448965974.json) has the same source fingerprint
as the clean build; the later commit only corrects executable file modes. These
software checks do not substitute for installation.

`os/tests/install_vm.py` is an explicit, non-root QEMU test. It creates a new
regular qcow2 target with no host disks, no shared directory and zero NICs.
Synthetic credentials and automated disk answers exist only in an additional
private test initrd; they are never added to the ISO. Generated private test
artifacts are ignored by Git. Do not publish them as release artifacts.
An auth-only retry uses a generated 0600 fixture secret inside the private 0700
test directory. It is removed on success; it is never a human account credential.
The boot probe requires actual PAM rejection/acceptance, password-required sudo,
the native login's own rejection/hidden input/recovery, the non-root tty1 writing
UI/input reader, matching installed executable hashes, logout to a fresh login,
effective disabled sleep and installed firmware/connection-tool payloads.
An interrupted or failed probe remains failed and retains partial diagnostics.

Still required: normal current-image installation completion, wrong-disk
refusal, earlier-stage interrupted installation/retry, encrypted install and
recovery, usable keyboard selection, physical firmware/Secure Boot checks,
input-safe lock/suspend, signed WrOS updates and complete redistribution/source
review. Do not deploy this development media as a production writing system.
