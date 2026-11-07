#!/bin/bash
# Build regular ISO artifacts only. Debian Installer owns the interactive disk,
# encryption and account workflow; this builder never selects a target disk.
#
# Profiles:
#   (default)  legacy Debian Installer media, audited and published here.
#   --live     the production live-session image. It stops after 'lb binary'
#              and leaves wros-live-amd64.hybrid.iso.capsule-less: that ISO has
#              no native-installer capsule and is NOT a release. The ordered
#              release driver (os/scripts/build-release-iso.sh) adds the capsule,
#              re-masters the ISO, audits the real artifact and publishes it.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
usage='Usage: build-installer.sh [--live] [--build-dir DIR], as root in an isolated Debian 13 builder.'
profile=installer
build_dir=
while [[ $# != 0 ]]; do
    case $1 in
        --live) profile=live-session; shift ;;
        --build-dir)
            [[ $# -ge 2 && $2 == /* ]] || { echo "$usage (DIR must be absolute)" >&2; exit 2; }
            build_dir=$2; shift 2 ;;
        *) echo "$usage" >&2; exit 2 ;;
    esac
done
[[ $EUID == 0 ]] || { echo 'Requires an isolated Debian 13 root builder.' >&2; exit 2; }
# Every prerequisite is checked here, before lb bootstrap spends minutes and
# gigabytes of network on a build that cannot finish.
missing=()
for command in lb debootstrap gpgv file zstd xorriso mksquashfs unsquashfs cpio python3 sha256sum; do
    command -v "$command" >/dev/null || missing+=("$command")
done
((${#missing[@]} == 0)) || { echo "Missing builder prerequisite(s): ${missing[*]}" >&2; exit 2; }
python3 -c 'import debian.debfile' || { echo 'Missing builder prerequisite: python3-debian' >&2; exit 2; }
# Debian 13 ships the binary keyring as .pgp; older layouts only have .gpg.
keyring=
for candidate in /usr/share/keyrings/debian-archive-keyring.pgp /usr/share/keyrings/debian-archive-keyring.gpg; do
    [[ -s $candidate ]] && { keyring=$candidate; break; }
done
[[ -n $keyring ]] || { echo 'Missing builder prerequisite: debian-archive-keyring (.pgp or .gpg)' >&2; exit 2; }
binary=${GRIMOIRE_BINARY:-$repo/target/debug/grimoire}
[[ -x $binary ]] || { echo 'Build Grimoire first or set GRIMOIRE_BINARY.' >&2; exit 2; }
greeter=${WROS_GREETER_BINARY:-$repo/target/debug/wros-greeter}
[[ -x $greeter ]] || { echo 'Build wros-greeter first or set WROS_GREETER_BINARY.' >&2; exit 2; }
image=wros-installer
volume=WROS_INSTALL
application='WrOS - Writer OS (experimental)'
bootappend='boot=live components live-config.noautologin'
if [[ $profile == live-session ]]; then
    installer=${WROS_INSTALLER_BINARY:-$repo/target/debug/wros-installer}
    [[ -x $installer ]] || { echo 'Build wros-installer first or set WROS_INSTALLER_BINARY.' >&2; exit 2; }
    image=wros-live
    volume=WROS_LIVE
    application='WrOS - Writer OS (release candidate)'
    bootappend='boot=live components wros.live=1'
fi
base=${build_dir:-$repo/build/$profile}
[[ ! -L $repo/build && ! -L $base && ! -L $(dirname "$base") ]] || { echo 'Refusing symlinked build paths.' >&2; exit 2; }
mkdir -p "$base"
shopt -s nullglob dotglob
entries=("$base"/*)
[[ ${#entries[@]} == 0 ]] || { echo 'Installer build directory must be empty; existing artifacts are preserved.' >&2; exit 2; }
cd "$base"
lb config --ignore-system-defaults --mode debian --distribution trixie \
    --architecture amd64 --archive-areas 'main non-free-firmware' \
    --binary-image iso-hybrid --bootloaders 'syslinux grub-efi' \
    --debian-installer live --debian-installer-distribution trixie \
    --debian-installer-gui false \
    --firmware-binary true --firmware-chroot false --apt-recommends false \
    --apt-secure true --security true --updates true --memtest none \
    --mirror-bootstrap https://deb.debian.org/debian \
    --mirror-chroot https://deb.debian.org/debian \
    --mirror-chroot-security https://security.debian.org/debian-security \
    --mirror-binary https://deb.debian.org/debian \
    --mirror-binary-security https://security.debian.org/debian-security \
    --mirror-debian-installer https://deb.debian.org/debian \
    --uefi-secure-boot enable --image-name "$image" \
    --iso-application "$application" --iso-volume "$volume" \
    --bootappend-live "$bootappend" \
    --bootappend-live-failsafe "$bootappend memtest noapic noapm nodma nomce nosmp nosplash vga=788" \
    --checksums sha256 --chroot-squashfs-compression-type zstd
# Authenticate the Debian Installer kernel/initrd (signed InRelease -> SHA256SUMS)
# before bootstrapping: the verified bytes are cached where live-build looks for
# them, and a missing keyring or unreachable mirror fails here, not an hour in.
python3 "$repo/tools/verify-debian-installer.py" cache/installer_debian-installer > installer-input-trust.json
if [[ $profile == installer ]]; then
    # --debian-installer-preseedfile takes a path on the medium, not on the builder. Debian
    # Installer loads /preseed.cfg from its own initrd, which config/includes.installer fills.
    mkdir -p config/includes.installer
    cp "$repo/os/config/installer.preseed" config/includes.installer/preseed.cfg
fi
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
    # live-build does not install Debian's launcher, yet wros-install-session sources
    # its plugins/live and debconf_kernel.sh from the running live system. The plugin
    # unpacks the installer initrd with cpio and stops its helpers with fuser (psmisc).
    # The launcher itself depends on debconf, menu, psmisc, xterm and sudo (all
    # in main); the audits pin the loaded launcher files by hash.
    printf 'debian-installer-launcher\ncpio\npsmisc\n' >> config/package-lists/wros.list.chroot
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
chroot chroot groupadd --system wros
WROS_GREETER_BINARY="$greeter" bash "$repo/os/scripts/install-files.sh" "$base/chroot" "$binary" installer
bash "$repo/os/scripts/stage-greeter.sh" "$base/chroot" "$greeter"
if [[ $profile == live-session ]]; then
    bash "$repo/os/scripts/stage-live-session.sh" "$base/chroot" "$installer"
    python3 "$repo/tools/audit-live-session.py" chroot > live-session-rootfs.json
fi
# Do not copy any VM-only getty/probe/account into the installed filesystem.
[[ ! -e chroot/etc/wros/VM-DEMO-ONLY && ! -e chroot/etc/systemd/system/getty@tty1.service.d/wros-vm.conf ]]
python3 "$repo/tools/audit-wifi-image.py" chroot > wifi-rootfs.json
lb installer
lb binary
iso="$image-amd64.hybrid.iso"
[[ -f $iso && ! -L $iso ]] || { echo "live-build did not produce $iso" >&2; exit 1; }
if [[ $profile == live-session ]]; then
    # WrOS installs from the live session; live-build's stock Debian Installer entries would
    # only install plain Debian. Rewrite the menus, then remake the checksums and the ISO
    # from the same tree, exactly as the release remaster does.
    python3 -I "$repo/tools/live-boot-menu.py" binary
    rm -f .build/binary_checksums .build/binary_iso "$iso"
    lb binary_checksums
    lb binary_iso
    [[ -f $iso && ! -L $iso ]] || { echo "live-build did not remake $iso" >&2; exit 1; }
    # Deliberately not a release: no capsule, no media audit, no release name.
    mv "$iso" "$iso.capsule-less"
    sha256sum "$iso.capsule-less" > "$iso.capsule-less.sha256"
    echo "live-build tree complete at $base; $iso.capsule-less has no native-installer capsule and is not a release." >&2
    exit 0
fi
# An ISO that has not passed the audit must never sit under its release name,
# and a failed audit must not leave an empty media-audit.json behind: the QEMU
# tests and manual flashing consume those names. Publish them only on success.
mv "$iso" "$iso.unaudited"
if ! python3 "$repo/tools/audit-installer-media.py" "$base" --profile installer --iso "$iso.unaudited" > media-audit.json.partial; then
    mv media-audit.json.partial media-audit.FAILED.json
    echo "Media audit failed; $iso.unaudited is NOT a usable image. See media-audit.FAILED.json." >&2
    exit 1
fi
mv media-audit.json.partial media-audit.json
mv "$iso.unaudited" "$iso"
sha256sum "$iso" > SHA256SUMS
echo 'Legacy Debian Installer media built and its payload audited. Installation, update and hardware qualification are separate gates.'
