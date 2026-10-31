#!/bin/bash
# Build regular ISO artifacts only. Debian Installer owns the interactive disk,
# encryption and account workflow; this builder never selects a target disk.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
profile=installer
if [[ $# == 1 && $1 == --live ]]; then
    profile=live-session
elif [[ $# != 0 ]]; then
    echo 'Usage: build-installer.sh [--live], as root in an isolated Debian 13 builder.' >&2; exit 2
fi
[[ $EUID == 0 ]] || { echo 'Requires an isolated Debian 13 root builder.' >&2; exit 2; }
for command in lb gpgv file zstd xorriso mksquashfs; do
    command -v "$command" >/dev/null || { echo "Missing builder prerequisite: $command" >&2; exit 2; }
done
python3 -c 'import debian.debfile' || { echo 'Missing builder prerequisite: python3-debian' >&2; exit 2; }
binary=${GRIMOIRE_BINARY:-$repo/target/debug/grimoire}
[[ -x $binary ]] || { echo 'Build Grimoire first or set GRIMOIRE_BINARY.' >&2; exit 2; }
greeter=${WROS_GREETER_BINARY:-$repo/target/debug/wros-greeter}
[[ -x $greeter ]] || { echo 'Build wros-greeter first or set WROS_GREETER_BINARY.' >&2; exit 2; }
image=wros-installer
volume=WROS_INSTALL
bootappend='boot=live components live-config.noautologin'
if [[ $profile == live-session ]]; then
    installer=${WROS_INSTALLER_BINARY:-$repo/target/debug/wros-installer}
    [[ -x $installer ]] || { echo 'Build wros-installer first or set WROS_INSTALLER_BINARY.' >&2; exit 2; }
    image=wros-live
    volume=WROS_LIVE
    bootappend='boot=live components wros.live=1'
fi
base="$repo/build/$profile"
[[ ! -L $repo/build && ! -L $base ]] || { echo 'Refusing symlinked build paths.' >&2; exit 2; }
mkdir -p "$base"
shopt -s nullglob dotglob
entries=("$base"/*)
[[ ${#entries[@]} == 0 ]] || { echo 'Installer build directory must be empty; existing artifacts are preserved.' >&2; exit 2; }
cd "$base"
lb config --ignore-system-defaults --mode debian --distribution trixie \
    --architecture amd64 --archive-areas 'main non-free-firmware' \
    --binary-image iso-hybrid --bootloaders 'syslinux grub-efi' \
    --debian-installer live --debian-installer-distribution trixie \
    --debian-installer-gui false --debian-installer-preseedfile "$repo/os/config/installer.preseed" \
    --firmware-binary true --firmware-chroot false --apt-recommends false \
    --apt-secure true --security true --updates true --memtest none \
    --mirror-bootstrap https://deb.debian.org/debian \
    --mirror-chroot https://deb.debian.org/debian \
    --mirror-chroot-security https://security.debian.org/debian-security \
    --mirror-binary https://deb.debian.org/debian \
    --mirror-binary-security https://security.debian.org/debian-security \
    --mirror-debian-installer https://deb.debian.org/debian \
    --uefi-secure-boot enable --image-name "$image" \
    --iso-application 'WrOS - Writer OS (experimental)' --iso-volume "$volume" \
    --bootappend-live "$bootappend" \
    --bootappend-live-failsafe "$bootappend memtest noapic noapm nodma nomce nosmp nosplash vga=788" \
    --checksums sha256 --chroot-squashfs-compression-type zstd
mkdir -p config/package-lists
cp "$repo/os/config/packages.txt" config/package-lists/wros.list.chroot
printf 'sudo\ngreetd\n' >> config/package-lists/wros.list.chroot
# Root installation alone does not populate Debian Installer's offline pool.
# The maintained binary-list stage downloads these packages and dependencies
# with an empty APT status database, then indexes them on the medium.
cp "$repo/os/config/encrypted-target-packages.txt" config/package-lists/wros-encryption.list.chroot
cp "$repo/os/config/encrypted-target-packages.txt" config/package-lists/wros-encryption.list.binary
if [[ $profile == live-session ]]; then
    printf 'live-boot\nlive-config\nlive-config-systemd\nuser-setup\n' >> config/package-lists/wros.list.chroot
fi
lb bootstrap
lb chroot
if [[ $profile == live-session ]]; then
    # The live session, its backend and the offline pool are all built for one
    # kernel ABI. An unpinned linux-image-amd64 can float past it after a Debian
    # point release; refuse here instead of failing at staging much later.
    abi=$(sed -n "/^ABI = '/{s/^ABI = '//;s/'$//;p}" "$repo/tools/build-live-installer-pool.py")
    [[ -n $abi ]] || { echo 'Cannot read the qualified live kernel ABI.' >&2; exit 1; }
    kernels=(chroot/boot/vmlinuz-*)
    [[ ${#kernels[@]} == 1 && ${kernels[0]} == "chroot/boot/vmlinuz-$abi" ]] || {
        echo "Live kernel drifted from the qualified $abi: ${kernels[*]}" >&2; exit 1; }
fi
chroot chroot groupadd --system writingdeck
bash "$repo/os/scripts/install-files.sh" "$base/chroot" "$binary" installer
bash "$repo/os/scripts/stage-greeter.sh" "$base/chroot" "$greeter"
if [[ $profile == live-session ]]; then
    bash "$repo/os/scripts/stage-live-session.sh" "$base/chroot" "$installer"
    python3 "$repo/tools/audit-live-session.py" chroot > live-session-rootfs.json
fi
# Do not copy any VM-only getty/probe/account into the installed filesystem.
[[ ! -e chroot/etc/writingdeck/VM-DEMO-ONLY && ! -e chroot/etc/systemd/system/getty@tty1.service.d/writingdeck-vm.conf ]]
python3 "$repo/tools/audit-wifi-image.py" chroot > wifi-rootfs.json
python3 "$repo/tools/verify-debian-installer.py" cache/installer_debian-installer > installer-input-trust.json
lb installer
lb binary
# An ISO that has not passed the audit must never sit under its release name,
# and a failed audit must not leave an empty media-audit.json behind: the QEMU
# tests and manual flashing consume those names. Publish them only on success.
iso="$image-amd64.hybrid.iso"
[[ -f $iso && ! -L $iso ]] || { echo "live-build did not produce $iso" >&2; exit 1; }
mv "$iso" "$iso.unaudited"
if ! python3 "$repo/tools/audit-installer-media.py" "$base" > media-audit.json.partial; then
    mv media-audit.json.partial media-audit.FAILED.json
    echo "Media audit failed; $iso.unaudited is NOT a usable image. See media-audit.FAILED.json." >&2
    exit 1
fi
mv media-audit.json.partial media-audit.json
mv "$iso.unaudited" "$iso"
sha256sum "$iso" > SHA256SUMS
echo 'Experimental media built. VM install/cancel/interruption and hardware qualification are still required.'
