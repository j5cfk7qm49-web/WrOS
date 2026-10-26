#!/bin/bash
# Assemble a UEFI GPT image from regular files. Never opens a physical disk.
# No loop devices, mount, repartitioning of the host, or grub-install.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
base="$repo/build/appliance"
root="$base/rootfs"
binary=${1:-$repo/target/debug/grimoire}
[[ $EUID == 0 && -f $root/etc/writingdeck/VM-DEMO-ONLY ]] || { echo 'Requires an explicit VM-demo rootfs and root for ownership preservation.' >&2; exit 2; }
for f in "$base/disk.img" "$base/root.ext4" "$base/efi.fat"; do
    [[ ! -L $f && ( ! -e $f || -f $f ) ]] || { echo "Refusing non-regular artifact: $f" >&2; exit 2; }
done
"$repo/os/scripts/install-files.sh" "$root" "$binary"
install -m 755 "$repo/os/tests/guest-boot-probe.py" "$root/usr/libexec/writingdeck-boot-probe"
install -m 644 "$repo/os/tests/writingdeck-boot-probe.service" "$root/etc/systemd/system/"
# These fixtures run only under the explicit VM-DEMO-ONLY profile. The probe
# invokes their application child as writer, never root. No host shares/NICs.
mkdir -p "$root/usr/libexec/writingdeck-vm-fixtures"
for fixture in guest-checks-probe.py test_checks_pty.py test_secondary_pty.py test_journal_pty.py test_productivity_pty.py terminal_screen.py; do
    install -m 644 "$repo/os/tests/$fixture" "$root/usr/libexec/writingdeck-vm-fixtures/"
done
ln -sf ../writingdeck-boot-probe.service "$root/etc/systemd/system/multi-user.target.wants/writingdeck-boot-probe.service"
# Use UUIDs rather than guessed /dev/sdX names; no dependency on disk ordering.
uuid=3224a43a-692d-4cd6-a526-96b9a630fa24
printf 'UUID=%s / ext4 defaults,noatime 0 1\n' "$uuid" > "$root/etc/fstab"
mkdir -p "$root/boot/grub"
kernel=$(find "$root/boot" -maxdepth 1 -name 'vmlinuz-*' -printf '%f\n' | sort -V | tail -1)
initrd=${kernel/vmlinuz/initrd.img}
cat > "$root/boot/grub/grub.cfg" <<GRUB
set default=0
set timeout=3
serial --unit=0 --speed=115200
terminal_output console serial
menuentry 'WritingDeck OS (isolated VM prototype)' {
    search --no-floppy --fs-uuid --set=root $uuid
    linux /boot/$kernel root=UUID=$uuid ro console=ttyS0,115200 console=tty1 quiet
    initrd /boot/$initrd
}
menuentry 'Recovery: authenticated maintenance boot' {
    search --no-floppy --fs-uuid --set=root $uuid
    linux /boot/$kernel root=UUID=$uuid ro systemd.unit=rescue.target
    initrd /boot/$initrd
}
GRUB
cat > "$base/early-grub.cfg" <<GRUB
search --no-floppy --fs-uuid --set=root $uuid
configfile /boot/grub/grub.cfg
GRUB
# Embed fallback bootloader for UEFI removable-media boot. Unsigned: Secure Boot
# is NOT claimed for this prototype. Production must retain Debian signed shim.
grub-mkstandalone -O x86_64-efi -o "$base/BOOTX64.EFI" \
    --modules='part_gpt fat ext2 normal configfile linux search search_fs_uuid serial' \
    "boot/grub/grub.cfg=$root/boot/grub/grub.cfg"
truncate -s 0 "$base/efi.fat"
truncate -s 128M "$base/efi.fat"
mkfs.vfat -F 32 -n WRITINGDECK "$base/efi.fat" > /dev/null
mmd -i "$base/efi.fat" ::/EFI ::/EFI/BOOT
mcopy -i "$base/efi.fat" "$base/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI
truncate -s 0 "$base/root.ext4"
truncate -s 2048M "$base/root.ext4"
mkfs.ext4 -F -q -U "$uuid" -L WRITINGDECK_ROOT -d "$root" "$base/root.ext4"
# 1 MiB alignment; fixed partition sizes. dd destinations are hardcoded regular
# artifacts checked above. sparse writes avoid storing gigabytes of zeroes.
truncate -s 0 "$base/disk.img"
truncate -s 2180M "$base/disk.img"
sgdisk --clear --new=1:2048:+128M --typecode=1:ef00 --change-name=1:EFI \
    --new=2:0:+2048M --typecode=2:8300 --change-name=2:WritingDeck "$base/disk.img" > /dev/null
efi_start=$(sgdisk -i 1 "$base/disk.img" | awk '/First sector:/ {print $3}')
root_start=$(sgdisk -i 2 "$base/disk.img" | awk '/First sector:/ {print $3}')
dd if="$base/efi.fat" of="$base/disk.img" bs=512 seek="$efi_start" conv=notrunc,sparse status=none
dd if="$base/root.ext4" of="$base/disk.img" bs=512 seek="$root_start" conv=notrunc,sparse status=none
sha256sum "$base/disk.img" > "$base/SHA256SUMS"
# Artifact custody: useful for running QEMU unprivileged. Guest file ownership
# was already embedded by mkfs.ext4 and remains root/writer appropriately.
if [[ -n ${SUDO_UID:-} ]]; then
    chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$base" "$repo/build" "$base/disk.img" "$base/SHA256SUMS"
fi
printf 'UEFI VM image: %s\n%s\n' "$base/disk.img" 'Run: python3 os/tests/boot_vm.py'
