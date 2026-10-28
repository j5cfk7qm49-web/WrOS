# Native live-installer image components

The live session and the Debian Installer chroot must use the same kernel.
The previous delivered image has live kernel `6.12.113+deb13-amd64` but installer
kernel `6.12.107+deb13-amd64`. It was qualified by booting the separate installer;
it cannot be treated as a qualified in-session live installer.

Debian Security already provides the matching signed kernel and installer udebs.
`os/scripts/build-live-backend.sh` uses Debian Installer source
`20250803+deb13u7` and its maintained build machinery to assemble a matching
`6.12.113+deb13-amd64` initrd. It adds the qualified cdebconf passthrough module
as a small local udeb, leaving Debian's partitioning, encryption, account,
copying and bootloader engines intact. The unprivileged native frontend remains
outside the installer chroot, in the live WrOS session.

## Component build

Provision the exact source descriptor, sibling source archive and
`linux-image-6.12.113+deb13-amd64_6.12.113-1_amd64.deb` through authenticated
Debian APT first. The recipe refuses different archive hashes, the wrong
architecture or an existing output directory. First build the module with
`os/scripts/build-installer-frontend.sh`; its manifest and patch must match.
Then run as a normal builder user:

```sh
bash os/scripts/build-live-backend.sh \
    /path/debian-installer_20250803+deb13u7.dsc \
    /path/linux-image-6.12.113+deb13-amd64_6.12.113-1_amd64.deb \
    /path/frontend-build /path/new-live-backend
```

The retained `wros-initrd` build profile excludes dependencies for documentation,
separate bootloader/Chromebook images and the old host kernel. The normal
upstream dependency check still runs for the selected component. Extra DRM
modules come from the extracted, pinned matching kernel package. No kernel
package is installed in the builder or host, and no target device is opened.

Debian's builder verifies its archive indexes and downloads the required udebs.
Its local package index trusts the explicitly built WrOS module; this is not an
official Debian-signed module. The output retains source, notices, the package
cache and a manifest with every udeb hash, recipe/source identity, compressed
initrd and kernel hashes. It verifies the module inside the compressed initrd
and checks that the output kernel bytes equal the signed Debian package bytes.
This records inputs and artifacts; it does not promise bit-identical rebuilds
against changing archive indexes or complete source/release-signing compliance.

## Remaining live integration

This recipe builds kernel/initrd components, not a complete live ISO. The stock
text frontend remains available in the component for recovery; the future WrOS
launcher must explicitly select passthrough with its supervised private channel.
Ordinary installation must stay in the shared Grimoire interface, including
errors, help, review and progress.

The live account and greetd startup override must exist only in the live boot
overlay. Debian live-installer 58 copies `/cdrom/live/filesystem.squashfs`, not
the running overlay; retain that boundary and verify the installed account and
authentication after reboot. Do not add live autologin to the copied SquashFS.

Still required are the supervised root launcher, one terminal/input owner,
private RAM answers, complete matching offline media/package pools, native disk
review, interruption/child cleanup, LUKS and fresh authenticated install/reboot
qualification. Earlier normal-install evidence cannot certify this new path.

The [actual build manifest](evidence/live-image-build.json) retains 103 udeb
hashes and compressed-artifact identities. The resulting image backend passed
[12 native form checks](evidence/live-image-backend-frontend.json) and
[14 fault cases](evidence/live-image-backend-faults.json) as a normal user with
private no-network/noexec-RAM fixtures. The
[source and driver record](evidence/live-image-local-provenance.json) and
[original failed attempts](evidence/live-image-attempts.json) retain their scope.
This component build is separate from the standard software CI gates; it is
not performed implicitly during every Rust test run.

[Debian CI 40673836623](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40673836623)
passed 972 Rust/configuration, 123 Python/zero skips, both Clippys and all 19
software gates. Its [artifact provenance](evidence/live-image-ci-provenance.json)
matches the final local component source and exact transport module. Earlier
passing records before the final configuration README update remain preserved
in [their original scope](evidence/live-image-before-readme/README.md).

The next [offline native/text package pool](NATIVE_INSTALLER_ENGINE.md) now
retains 401 authenticated packages, both required kernel ABIs and strict
dependency availability. It is a regular-file input for the pending privileged
integration; the earlier component build and live ISO keep their own scopes.
