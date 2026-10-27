# Experimental offline Debian Installer media

This continues the existing Debian 13 appliance. The separate VM demo remains
available; it is not an installation image. The new ISO uses Debian's maintained
`live-build` and `live-installer` to copy the complete writing filesystem, then
uses Debian Installer for storage, encryption, accounts and the bootloader.
See the [Debian Live installer documentation](https://live-team.pages.debian.net/live-manual/html/live-manual/customizing-installer.en.html)
and [Debian account/storage options](https://www.debian.org/releases/trixie/amd64/apbs04.en.html).

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
the failed probe as passed or claiming that credentials were tested.

## Build

Use an isolated Debian 13 amd64 builder on a Linux filesystem with enough room
for chroot/cache/ISO and disposable test disks. Install `live-build debootstrap
squashfs-tools xorriso isolinux syslinux-common grub-pc-bin grub-efi-amd64-bin
cpio file zstd rsync gpgv debian-archive-keyring` plus the appliance build
prerequisites in [INSTALLATION](INSTALLATION.md). Build the existing Grimoire
binary and exact Python runtime first, as described in [M7B](M7B.md).

```sh
sudo env GRIMOIRE_BINARY=/absolute/path/to/grimoire \
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
A clean end-to-end rerun of that revised recipe remains a qualification gate.

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
ShellCheck and Python syntax checks pass. These do not substitute for installation.

`os/tests/install_vm.py` is an explicit, non-root QEMU test. It creates a new
regular qcow2 target with no host disks, no shared directory and zero NICs.
Synthetic credentials and automated disk answers exist only in an additional
private test initrd; they are never added to the ISO. Generated private test
artifacts are ignored by Git. Do not publish them as release artifacts.

Still required: finished VM install and authenticated first boot, cancellation,
no-disk/wrong-disk refusal, interrupted installation/retry, encrypted install and
recovery, usable keyboard selection, physical firmware/Secure Boot checks,
input-safe lock/suspend, signed WrOS updates and complete redistribution/source
review. Do not deploy this development media as a production writing system.
