#!/bin/bash
# Build regular ISO artifacts only. Debian Installer owns the interactive disk,
# encryption and account workflow; this builder never selects a target disk.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
[[ $# == 0 && $EUID == 0 ]] || { echo 'Run without arguments as root in an isolated Debian 13 builder.' >&2; exit 2; }
for command in lb gpgv file zstd xorriso mksquashfs; do
    command -v "$command" >/dev/null || { echo "Missing builder prerequisite: $command" >&2; exit 2; }
done
binary=${GRIMOIRE_BINARY:-$repo/target/debug/grimoire}
[[ -x $binary ]] || { echo 'Build Grimoire first or set GRIMOIRE_BINARY.' >&2; exit 2; }
base="$repo/build/installer"
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
    --uefi-secure-boot enable --image-name wros-installer \
    --iso-application 'WrOS - Writer OS (experimental)' --iso-volume WROS_INSTALL \
    --bootappend-live 'boot=live components live-config.noautologin' \
    --checksums sha256 --chroot-squashfs-compression-type zstd
mkdir -p config/package-lists
cp "$repo/os/config/packages.txt" config/package-lists/wros.list.chroot
printf 'sudo\ncryptsetup\ncryptsetup-initramfs\n' >> config/package-lists/wros.list.chroot
lb bootstrap
lb chroot
chroot chroot groupadd --system writingdeck
bash "$repo/os/scripts/install-files.sh" "$base/chroot" "$binary" installer
# Do not copy any VM-only getty/probe/account into the installed filesystem.
[[ ! -e chroot/etc/writingdeck/VM-DEMO-ONLY && ! -e chroot/etc/systemd/system/getty@tty1.service.d/writingdeck-vm.conf ]]
python3 "$repo/tools/audit-wifi-image.py" chroot > wifi-rootfs.json
python3 "$repo/tools/verify-debian-installer.py" cache/installer_debian-installer > installer-input-trust.json
lb installer
lb binary
python3 "$repo/tools/audit-installer-media.py" "$base" > media-audit.json
sha256sum wros-installer-amd64.hybrid.iso > SHA256SUMS
echo 'Experimental media built. VM install/cancel/interruption and hardware qualification are still required.'
