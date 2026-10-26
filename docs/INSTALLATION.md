# Development, image building and safe installation

## Safety first

There is **no production disk installer yet**. Nothing in this repository authorizes erasing a physical disk. The current image profile is an isolated, passwordless **VM demo**, not a hardware deployment image. No physical repartitioning/formatting was performed during development.

`install-plan.py` performs read-only inventory and validates a proposed disk, account, IANA timezone, encryption preference and registered keyboard. It has deliberately **no `--apply` mode**. `install-files.sh` stages integration into an existing Debian root and explicitly refuses `/`. These are testable installer building blocks, not a complete wizard.

## Conventional Linux development (no system policy installation)

Build as a normal user. Install a maintained Rust toolchain capable of the Rust 2024 dependencies; the test run used Rust 1.99.1. Debian's packaged toolchain may be older than the current lockfile requires. Prefer a reviewed/pinned toolchain in release CI. Building default audio on Debian also needs `build-essential pkg-config libasound2-dev`.

```sh
cargo test --workspace --locked -j 2
cargo run --locked --no-default-features -- /path/to/existing/book
# or create a new project through Grimoire's existing new-book command
cargo run --locked --no-default-features -- new /path/to/new/book
```

F12 reaches WritingDeck without changing your desktop keyboard, login, NetworkManager configuration or power state. Status reads may report missing system services. Actions occur only when explicitly requested and authorized by the host. Do not invoke a power action merely as a development test; use the simulator.

```sh
cargo run --locked --no-default-features -- writingdeck status --simulate
cargo run --locked --no-default-features -- writingdeck check-config /path/to/book
cargo run --locked --no-default-features -- writingdeck tutorial-demo /path/to/NEW-demo
```

The tutorial creator refuses every existing target directory, including empty ones. No cloud credentials are needed. Once dependencies are fetched, the Rust tests/build can use `--offline`; first-time package/source downloads obviously need a repository or a prepared offline cache.

## Build the complete UEFI VM disk

Tested builder host: Debian 13 amd64, approximately 2 GiB RAM / 2 CPUs. Allow several GiB of free storage. Install build prerequisites (these are host packages, not a desktop in the guest):

```sh
sudo apt-get install --no-install-recommends \
  build-essential pkg-config libasound2-dev mmdebstrap qemu-system-x86 qemu-utils \
  ovmf gdisk dosfstools mtools e2fsprogs grub-efi-amd64-bin \
  console-setup kbd xkb-data python3 python3-pil shellcheck
cargo build --locked --no-default-features -j 2
sudo os/scripts/bootstrap-rootfs.sh --vm-demo
sudo os/scripts/assemble-image.sh
python3 os/tests/boot_vm.py
```

The rootfs builder refuses an existing rootfs. The assembler reconstructs only hardcoded **regular build artifacts**; it refuses symlinks/block devices and truncates artifacts before sparse reassembly to prevent stale bytes. It never mounts or formats a host block device. Grimoire is copied/stripped as a file by root, then actually executed only as the unprivileged writer inside the VM.

Outputs under `build/appliance/`: staged rootfs, `disk.img` (GPT EFI + ext4), requested/resolved package lists, image SHA-256, firmware-variable copy, serial logs and screenshot. The rootfs uses maintained Debian kernel/initramfs packages. The QEMU acceptance test boots via OVMF/GRUB, uses a disposable COW layer, two virtual CPUs, 512 MiB RAM and **zero network devices**. It reads the actual tty1 screen, not just a successful `--version` exit.

Generated artifacts are intentionally ignored/excluded from source snapshots. The small evidence under `docs/evidence/` is retained. `DEBIAN_MIRROR` can select a reviewed snapshot repository; a deterministic recipe is not yet a bit-identical, pinned, signed release.

## Physical laptop path (manual upstream installation; not executed here)

The WrOS image recipe includes Debian's kernel wireless modules and firmware for
Intel, Realtek, Qualcomm/Atheros (including ath9k USB), Broadcom/Cypress using
brcm80211, and MediaTek. `main non-free-firmware` stays enabled for installed
security/firmware updates. NetworkManager, `wpasupplicant`, regulatory data,
`rfkill`, `iw` and the `nmtui` connection interface are installed; connecting must
not depend on first downloading a Wi-Fi firmware package. The assembler rejects
images missing these installed packages, firmware payloads, notices, wireless
modules or `nmtui` and records `wifi-payload.json`.

The future production installer media must carry the same firmware package set,
including when installation has no Internet. Use Debian's firmware-inclusive
installer rather than a network-only image. Debian documents the inclusion and
installed-system handling in its [firmware guidance](https://wiki.debian.org/Firmware).
The policy is tracked in `os/config/wifi-firmware.json`.

Firmware supports particular kernel drivers; some Broadcom and very new adapters
need additional drivers or a newer supported kernel. Physical acceptance must
record chipset/PCI or USB ID, firmware, connection, reboot and suspend/resume.
Bundling packages is not a claim that every laptop adapter has been tested.

Until a WritingDeck installer is implemented and qualified, use Debian's maintained installer for storage, firmware, LUKS, accounts and bootloader. Back up independently and verify restoration first. Select the intended disk only after reviewing its model/serial/partitions. Use the installer's explicit destructive confirmation; this planner cannot provide permission.

Recommended production decisions, still requiring integration tests:

1. amd64 laptop compatibility and necessary non-free firmware reviewed explicitly; no blanket driver-compatibility claim.
2. UEFI and Debian's signed shim/GRUB/kernel chain retained when Secure Boot is required. The prototype's unsigned standalone GRUB does **not** support that claim. Legacy BIOS is not tested.
3. LUKS-encrypted writable storage with separately held recovery secrets; disk encryption does not replace a session lock.
4. A real non-root password/PAM account, authenticated tty2 maintenance login and a tested lock-before-suspend workflow. Do not copy the VM autologin drop-in or its locked-account configuration.
5. Timezone/clock and console layout selected before normal text entry. First-run in-app onboarding and custom layout registration remain unfinished.
6. NetworkManager for optional Wi-Fi; `nmtui` at the maintenance console can establish profiles until in-app credential entry is safely implemented. Never store passwords in command arguments, shell history or plans.
7. A packaged, reviewed installation of the session wrapper, keymap units, exact helper policy and app. The current staging script is not a live installer.
8. Test update failure, recovery boot, lock/suspend, brightness, keyboard recovery and backup restore on the actual machine before using it as the only copy of a novel.

## Maintenance and recovery

Ctrl+Alt+F2 reaches a normal authenticated getty on a real installation. Check services with `systemctl status writingdeck-keyboard.service` and logs with `journalctl -u writingdeck-keyboard.service`. Use an administrator to reapply QWERTY or repair `/etc/writingdeck/keyboard.json`; Grimoire itself never becomes root.

In the disposable VM, passwords are locked, so normal authenticated recovery login is intentionally unavailable. Recovery menu structure is present, but usable production recovery authentication is a release gate. The automated probe powers the VM off after its assertions. Remove the VM-only probe from any future interactive/hardware image; never silently turn this test image into a deployment image.

## Updates and offline behavior

Use Debian apt/signatures and retain a known-good kernel; no custom package manager. In-app update staging, offline signed bundles and snapshot/rollback automation are **Planned**, not implemented by the image builder. Keep novel data separate from OS snapshots. A failed package download must not affect offline writing. The prototype does not promise automatic rollback or silently run unattended updates while you type.

## M3b application check after a development build

F11 chooses an independent editor; both documents use the existing offline save/history/recovery path. At 80×24 they stack with the outline visible; wider writing areas split horizontally. Ctrl-D always focuses Primary only. Schema-v1 layouts migrate only in memory until an explicit guarded save. See [WORKSPACES](WORKSPACES.md). The Python PTY test launches the real compiled application as the current non-root user in four fresh demo fixtures; it must skip rather than run interactively as root. The separate VM probe remains a boot/system/console-keyboard check, not a claim that its scripted guest exercises F11.
