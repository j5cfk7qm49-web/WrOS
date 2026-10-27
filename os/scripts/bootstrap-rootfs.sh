#!/bin/bash
# Writes a new build directory only. No block device, mount, grub-install, or
# host account edits. The explicit --vm-demo flag permits an insecure VM login.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
if [[ ${1:-} != --vm-demo ]]; then
    echo 'Prototype only: pass --vm-demo for an isolated, networkless QEMU image.' >&2
    echo 'Production installation uses the authenticated Debian installation path in docs/INSTALLATION.md.' >&2
    exit 2
fi
[[ $EUID == 0 ]] || { echo 'mmdebstrap requires root; only this image builder, never Grimoire.' >&2; exit 1; }
base="$repo/build/appliance"
root="$base/rootfs"
mkdir -p "$base"
[[ ! -e $root ]] || { echo "$root already exists; refusing to overwrite. Use a new checkout/build directory." >&2; exit 1; }
mirror=${DEBIAN_MIRROR:-https://deb.debian.org/debian}
packages=$(paste -sd, "$repo/os/config/packages.txt")
# Optional pinned snapshot mirror can be supplied; record the actual resolved
# packages below. Default includes current trixie security packages.
sources="deb $mirror trixie main non-free-firmware"
if [[ $mirror == https://deb.debian.org/debian ]]; then
    sources+=$'\ndeb https://security.debian.org/debian-security trixie-security main non-free-firmware'
    sources+=$'\ndeb https://deb.debian.org/debian trixie-updates main non-free-firmware'
fi
mmdebstrap --mode=root --variant=minbase --architectures=amd64 \
    --include="$packages" --aptopt='Acquire::Retries "3"' trixie "$root" "$sources"
# Stop service startup in the build chroot, not on the host.
printf '#!/bin/sh\nexit 101\n' > "$root/usr/sbin/policy-rc.d"
chmod 755 "$root/usr/sbin/policy-rc.d"
printf 'writingdeck-vm\n' > "$root/etc/hostname"
printf '127.0.0.1 localhost\n127.0.1.1 writingdeck-vm\n' > "$root/etc/hosts"
printf 'C.UTF-8\n' > "$root/etc/locale.conf"
printf 'Etc/UTC\n' > "$root/etc/timezone"
ln -sf /usr/share/zoneinfo/Etc/UTC "$root/etc/localtime"
chroot "$root" groupadd -r writingdeck
chroot "$root" useradd -m -s /bin/bash -G writingdeck writer
chroot "$root" passwd -l root
chroot "$root" passwd -l writer
# $TERM is expanded by systemd, not the builder.
# shellcheck disable=SC2016
printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin writer --noclear %%I $TERM\n' > "$base/getty-demo.conf"
mkdir -p "$root/etc/systemd/system/getty@tty1.service.d" "$root/etc/writingdeck" \
    "$root/usr/libexec" "$root/usr/share/writingdeck/keymaps" "$root/usr/share/doc/writingdeck" \
    "$root/etc/polkit-1/rules.d" "$root/usr/share/polkit-1/actions"
install -m 644 "$base/getty-demo.conf" "$root/etc/systemd/system/getty@tty1.service.d/writingdeck-vm.conf"
printf 'ISOLATED VM DEMO: PASSWORDLESS AUTOLOGIN. DO NOT DEPLOY TO HARDWARE.\n' > "$root/etc/writingdeck/VM-DEMO-ONLY"
printf 'WritingDeck OS development appliance\nDebian 13 / trixie; VM demo only\n' > "$root/etc/issue"
# dpkg-query owns these field expansions.
# shellcheck disable=SC2016
chroot "$root" dpkg-query -W '-f=${Package}\t${Version}\t${Architecture}\n' > "$base/packages.lock.tsv"
cp "$repo/os/config/packages.txt" "$base/requested-packages.txt"
rm -f "$root/usr/sbin/policy-rc.d"
printf 'Rootfs ready: %s\nNext: os/scripts/assemble-image.sh\n' "$root"
