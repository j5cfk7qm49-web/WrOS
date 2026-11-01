# WrOS Rescue Guide (Release Candidate)

This document describes the rescue and repair procedures for an installed WrOS system.

## 1. Design & Scope

### ext4 Filesystem and Rollback
- WrOS installs onto an ext4 root filesystem on an encrypted LUKS partition (optionally within an LVM volume group).
- **ext4 does not provide filesystem-level snapshot rollback** (such as Btrfs or ZFS snapshots).
- WrOS does not claim snapshot rollback capability.
- Recovery focuses on **package transaction repair and crash consistency**: fixing interrupted dpkg states, completing pending half-configured packages, and reconciling offline update journals using strictly authenticated retained packages.
- Repair does not undo a successfully completed update; it repairs broken, interrupted, or inconsistent system states.

## 2. In-System Recovery vs. Live ISO Rescue

WrOS provides two tiers of repair:

### Tier 1: In-System Repair (Settings > Recovery > Repair)
When the installed system boots to a login:
- After a failed or unverified offline update the system does not reboot by itself. It
  continues to a normal login, and `wros-update-reconcile.service` runs before the login
  screen. It records an interrupted update as interrupted and attempts one bounded repair.
- In Settings > Recovery, Repair runs the privileged entry point
  `/usr/libexec/wros-update-repair` through polkit (`pkexec`). The launcher refuses to run
  without a polkit caller, so a root shell cannot invoke it directly. From a local console
  session of a `writingdeck` group member, `pkexec /usr/libexec/wros-update-repair` is
  equivalent.
- Repair uses only the retained update archives in `/var/lib/wros-update/staged/`, after
  re-authenticating them against the current keyring and signer pin. It runs
  `dpkg --configure -a`, then installs the retained archives if dpkg is still inconsistent.
- Journal records are appended to `/var/lib/wros-update/journal.jsonl` (root-only). The
  readable summary that Settings shows is `/var/lib/wros-update/public/status.json`.

### Tier 2: Live ISO Rescue (`wros-rescue`)
When the installed system fails to boot (e.g. boot-loader corruption, interrupted kernel upgrade, or initramfs misconfiguration):
- Boot the system using the WrOS Live ISO / installation media. The live image carries the
  tool as `/usr/sbin/wros-rescue`. It refuses to run outside the live system.
- Open a terminal session on the live environment (for example Ctrl+Alt+F2).
- Run:
  ```bash
  sudo wros-rescue [--device /dev/sdXN]
  ```

## 3. Live ISO Rescue Workflow (`wros-rescue`)

The `wros-rescue` script automates the safe mounting, chroot execution, and cleanup process:

1. **Pre-flight Checks**:
   - Confirms root execution (`EUID == 0`).
   - Verifies the live environment (`/proc/cmdline` contains `boot=live`).
   - Ensures an interactive controlling terminal (`/dev/tty`) for secure passphrase input.

2. **LUKS Partition Detection & Unlocking**:
   - Scans block devices (`lsblk`) for partitions with type `crypto_LUKS`.
   - If multiple LUKS partitions are detected, prompts the operator to specify `--device /dev/sdXN`.
   - Prompts for the disk encryption passphrase interactively. The passphrase is typed directly to `cryptsetup` and is never saved or logged.
   - Unlocks the container as `/dev/mapper/wros-rescue-root`.

3. **LVM & Root Identification**:
   - If LVM is present, activates the volume group (`vgchange -ay <vg>`).
   - Inspects candidate ext4 volumes and mounts read-only to verify system identity:
     - Checks `/etc/writingdeck/release` exists.
     - Confirms `/etc/crypttab` references the matching LUKS UUID.
   - Requires exactly one matching WrOS root volume.

4. **Private Target Mounting & Chroot Preparation**:
   - Mounts the target filesystem read-write under a private target `/target` (`mount -t ext4 -o rw,nodev`).
   - Bind-mounts kernel pseudo-filesystems:
     - `/dev` -> `/target/dev`
     - `/dev/pts` -> `/target/dev/pts`
     - `/proc` -> `/target/proc`
     - `/sys` -> `/target/sys`
     - `/run` -> `/target/run` (tmpfs)
   - Mounts `/boot/efi` inside the chroot if an EFI System Partition is defined in `/etc/fstab`.

5. **Executing Update Repair**:
   - Verifies presence of the installed system's update engine (`/usr/lib/wros-update/engine.py` and `/usr/libexec/wros-update`).
   - Executes inside the chroot:
     ```bash
     env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin LC_ALL=C WROS_RESCUE=1 /usr/libexec/wros-update rescue-repair
     ```
   - Re-authenticates retained update archives against `/usr/share/keyrings/wros-archive-keyring.gpg` and pinned fingerprints in `/usr/share/wros/update-signers`.
   - Completes interrupted dpkg configuration and regenerates initramfs / boot configuration if necessary.

6. **Automatic Cleanup & Teardown**:
   - Regardless of whether the repair succeeds or fails (via `trap ... EXIT`):
     - Unmounts all bind mounts and `/boot/efi` in reverse order.
     - Unmounts `/target`.
     - Deactivates LVM volume groups (`vgchange -an`).
     - Closes the LUKS mapping (`cryptsetup close wros-rescue-root`).
   - Leaves no mounts or decrypted cryptographic devices open.

## 4. Manual Fallback Procedure

If `wros-rescue` cannot run automatically (e.g. if `/usr/lib/wros-update/engine.py` is damaged), the operator can perform manual recovery from the Live ISO:

1. Unlock the LUKS partition:
   ```bash
   sudo cryptsetup open /dev/sdXN wros-manual-root
   ```
2. If LVM is used:
   ```bash
   sudo vgscan && sudo vgchange -ay
   ```
3. Mount root and pseudo-filesystems:
   ```bash
   sudo mkdir -p /target
   sudo mount /dev/mapper/<root-volume> /target
   sudo mount --bind /dev /target/dev
   sudo mount --bind /dev/pts /target/dev/pts
   sudo mount -t proc proc /target/proc
   sudo mount -t sysfs sysfs /target/sys
   sudo mount -t tmpfs tmpfs /target/run
   sudo mount /dev/sdX1 /target/boot/efi  # if EFI system partition exists
   ```
4. Enter chroot:
   ```bash
   sudo chroot /target /bin/bash
   ```
5. Inside chroot, diagnose and repair dpkg:
   ```bash
   dpkg --configure -a
   apt-get install -f --no-download   # offline: only packages already on the system
   update-initramfs -u -k all
   update-grub
   exit
   ```
6. Clean up:
   ```bash
   sudo umount -R /target
   sudo vgchange -an  # if LVM used
   sudo cryptsetup close wros-manual-root
   ```
