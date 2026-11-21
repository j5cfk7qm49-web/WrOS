# WrOS Rescue Guide (Release Candidate)

This document describes the rescue and repair procedures for an installed WrOS system.

## 1. Design & Scope

### ext4 Filesystem and Rollback
- WrOS uses a FAT32 EFI System Partition labelled `WROS-ESP` and a LUKS2 partition labelled `WROS-LUKS`, opened as `wros_crypt`, containing the ext4 root labelled `WROS-ROOT`. The NixOS installer does not create LVM.
- **ext4 does not provide filesystem-level snapshot rollback** (such as Btrfs or ZFS snapshots).
- WrOS does not claim snapshot rollback capability.
- Repair checks Nix store integrity and re-activates the recorded installed generation or the current system-profile generation for boot. It does not run dpkg or restore damaged store contents automatically; a failed integrity check is reported.
- Repair does not undo a successfully completed update.
- **Going back** stages a switch to the retained previous NixOS system generation for the next restart. It does not reinstall Debian packages. Probation can stage the same rollback automatically after two failed writing-session starts.

## 2. In-System Recovery vs. Live ISO Rescue

WrOS provides three tiers of repair:

### Tier 1: In-System Repair (Settings › Updates and recovery › Recovery)
When the installed system boots to a login:
- After a failed or unverified offline update the system does not reboot by itself. It
  continues to a normal login, and `wros-update-reconcile.service` runs before the login
  screen. It records interrupted transactions, reconciles probation and publishes update
  status. Repair is a separate action.
- In Settings › Updates and recovery › Recovery, Repair runs the privileged entry point
  `wros-update-repair` through polkit (`pkexec`). The launcher refuses to run without a polkit
  caller, so a root shell cannot invoke it directly. Polkit allows it only by its store path,
  so from a local console session of a `wros` group member the equivalent is
  `pkexec "$(readlink -f /run/current-system/sw/libexec/wros-update-repair)"`.
- Repair checks Nix store integrity and re-activates the recorded installed generation or
  the current system-profile generation for boot. It does not run dpkg or restore damaged
  store contents automatically; a failed integrity check is reported.
- Journal records are appended to `/var/lib/wros-update/journal.jsonl` (root-only). The
  readable summary that Settings shows is `/var/lib/wros-update/public/status.json`.

### Tier 2: The WrOS recovery boot (from rc5)
When the normal session will not start, but the laptop still boots:
- The boot menu has a **WrOS recovery** entry. The menu is hidden on a normal boot (press Esc
  in the first 2 seconds to show it). After a boot that never reached the login screen it
  shows by itself for 30 seconds. An offline-update boot counts as a good boot.
  NixOS emergency and rescue shells are masked.
- The entry boots the selected generation's recovery specialisation into
  `wros-recovery.target`, with no login screen and no network. After the disk passphrase,
  the menu on tty1 asks for the writer's own password (PAM
  service `wros-recovery`; members of the `wros` group only). Three wrong passwords
  pause it for 30 seconds.
- **Choices:**
  - go back to the kept version (installs when you restart);
  - repair system packages (the same bounded repair as Tier 1);
  - restore a project from the account's backup into a new folder under `~/Restored books`. A
    Drive backup asks before starting the network.
  - show what happened: the update status and the previous boot's errors;
  - continue to the normal login;
  - continue in the console session until the next restart, when a graphical session is selected;
  - restart;
  - power off.
- It never offers a shell; Ctrl+C is ignored, and systemd restarts the menu if it ever exits.
  Restoring onto a fresh installation with the recovery key stays in Settings › Backups.
- GRUB also retains earlier NixOS generations, each with its own kernel and initrd.

### Tier 3: Live ISO Rescue (`wros-rescue`)
When the installed system fails to boot (e.g. boot-loader corruption, interrupted kernel upgrade, or initramfs misconfiguration):
- Boot from the WrOS USB stick and choose **WrOS rescue** in its boot menu.
- Enter the installed disk's passphrase when prompted. The rescue entry runs `wros-rescue`
  as root on the console and starts with a repair attempt.
- When the attempt ends, choose whether to run rescue again, roll back to the previous
  generation, or power off. If you choose rollback, enter the disk passphrase again.

## 3. Live ISO Rescue Workflow (`wros-rescue`)

The helper requires root and an interactive terminal on a live boot (`boot=live`
or `wros.live=1`). It prefers `WROS-LUKS`, otherwise requires one LUKS candidate
or `--device`.

It opens `wros_crypt`, mounts the ext4 root (`WROS-ROOT` when present) read-write
at `/target`, checks `/etc/wros/release`, mounts `WROS-ESP` at `/target/boot` when
present, and prepares `/dev`, `/dev/pts`, `/proc`, `/sys` and a temporary `/run`.

It runs the installed NixOS repair engine, or uses `--rollback` to select the
previous system-profile generation and regenerate its boot entries. Exit cleanup
unmounts its mounts and closes the mapping. It does not activate LVM, rename
mappings from crypttab, mount `/boot/efi`, or repair dpkg.
[Rescue helper](../nixos/scripts/wros-rescue)

## 4. Manual Fallback Procedure

First try a retained generation in GRUB: each earlier NixOS generation boots with its own
kernel and initrd. Then try the recovery boot (Tier 2). Debian package commands (dpkg, apt,
update-initramfs, update-grub) do not apply to NixOS and must not be run. If neither works,
note the error and reinstall from the stick, then restore your projects from a verified backup.
`wros-update-rollback` stages the same switch to a retained generation from a running system.
See [`NIXOS_UPDATES.md`](NIXOS_UPDATES.md).
