# Per-machine firmware (rc33)

Status: approved by the owner, 2026-11-27.

## Why

rc32 split firmware into one package per kernel driver and put only the HP Stream 11's into the
installed system. That keeps the system small and inside GitHub Pages' 100 MB per-file limit, but
any other laptop, or a USB Wi-Fi adapter, loses its firmware after an update. The full
linux-firmware is about 1.9 GB, too much for a 32 GB eMMC.

rc33 gives each laptop the firmware its own hardware needs. The signed system stays the same for
everyone. The owner asked for it to be "as uniform and efficient as possible, graphics included":
one path for all firmware, early-boot graphics too, and nothing compiled on the laptop.

## Goals

- Every installed WrOS loads all of its firmware the same way, from a per-machine set chosen from
  its own hardware, in early boot (initrd) and after.
- The set is chosen at install, re-checked at every update, and can be extended later for a
  device plugged in afterwards.
- Firmware downloads are signed and verified like the rest of an update, and fetch only what
  this laptop needs.
- Go back restores the firmware that went with the version you go back to.
- The live ISO still carries every package, so an install works offline.

## Non-goals

- NVIDIA. Its firmware is 155 MB, the GSP files alone far over Pages' per-file limit, and
  low-end laptops with an NVIDIA chip also have an Intel or AMD one that drives the screen.
- Picking single chips inside a driver (only the AMD GPU generation in use, for example). A
  driver's package is the unit, as in rc32. The Stream keeps its trimmed Gemini Lake i915
  package.
- Building anything on the laptop.

## Anti-goals

- A laptop that boots after an update with no Wi-Fi or a black screen because its firmware
  didn't come along. That's the one outcome that makes this a failure even if it works
  elsewhere.
- Firmware that changes the signed system's identity (its toplevel): every laptop on a release
  must still run the same system.

## How it works

### 1. Driver packages (build time)

rc32's `nixos/modules/firmware.nix` already builds one package per driver from committed file
lists. rc33 adds these families:

| Package | Driver(s) | Size before compression |
|---|---|---|
| amdgpu | amdgpu | 90 MB |
| radeon | radeon | 5 MB |
| xe | xe (new Intel graphics) | 6 MB |
| bt-intel, bt-realtek, bt-qca, bt-broadcom | btusb, btintel, btrtl, btqca, btbcm, hci_uart | 1–19 MB each |
| sof | the SOF sound firmware and topologies (now in every system) | from sof-firmware |

Existing ones stay: the Wi-Fi drivers, i915 and the Stream's i915-glk. regdb stays in the
system for everyone: it's tiny and every Wi-Fi card needs it.

Each package's store path is also exported in a table, `driver package → kernel module names`,
built from the same module globs, so a laptop can map a module to its package without Nix.

The system itself no longer carries any of these in `hardware.firmware`.

### 2. Detection (on the laptop)

A small root tool, `wros-firmware`, reads every `/sys/bus/*/devices/*/modalias`, resolves each
alias with `modprobe --resolve-alias` against a given system's kernel modules (`-d`, `-S`), adds
the modules already loaded, and maps the module names to packages with the table. The result is
a list of package names. It never loads anything.

It runs:
- in the installer, against the system being installed;
- in the update engine, against the new system's kernel, before staging;
- from a Settings row, *Firmware for a new device*, for something plugged in later;
- as a fallback, at bootloader install and at activation, if a system has no set yet (see the
  transition below).

### 3. The per-machine set, one per system generation

For each installed system generation, `wros-firmware` writes a folder of symlinks:

```
/nix/var/wros/firmware/<system name>/lib/firmware/<file> -> /nix/store/<package>/lib/firmware/<file>
```

where `<system name>` is the generation's toplevel store name. Each package it uses gets a GC
root under `/nix/var/nix/gcroots/wros-firmware/`. A folder whose system generation is gone is
removed with its roots when the engine deletes old generations.

Keying by system generation is what makes go back work with no special case: the previous
generation still has its folder, so booting it uses its own firmware. It also covers a kernel
update that asks for new firmware file names.

### 4. Loading it, the same way early and late

- **After boot:** NixOS points the kernel's custom firmware path at the system's own
  `hardware.firmware` (now only regdb). The kernel's built-in default path, `/lib/firmware`, is
  unused on NixOS. Activation makes `/lib/firmware` a symlink to the running generation's folder
  (it knows its toplevel as `$systemConfig`). The kernel follows symlinks when it loads firmware.
- **Early boot:** GRUB already loads a second initrd for each generation when the system has an
  `append-initrd-secrets` script. WrOS replaces that script (`system.build.initialRamdiskSecretAppender`)
  with one that packs that generation's firmware into a cpio with the files at
  `lib/firmware/…`, resolving the symlinks. It packs only the packages whose drivers load in the
  initrd (the graphics ones), so `/boot` holds a few MB per generation on the Stream. The kernel unpacks it into its root before init runs,
  so i915's display firmware is there when i915 loads in the initrd. The script finds its
  generation from the path it was run from. No compiling, only `cpio` and `zstd`, which the system
  already has. This is the same mechanism the Plymouth themes will use in rc34.

### 5. Delivery in an update

The signed closure manifest gets a new section, `firmware`, listing every driver package for that
release with its store path and its cache files (narinfo and nar, with SHA-256 and size), all
hosted on Pages. Older engines ignore it.

The rc33+ engine:
1. downloads the system update as now;
2. runs detection against the new system;
3. downloads only the packages it needs and doesn't have, verifying each against the signed
   manifest (with #179's retry and resume);
4. imports them into the store with the system;
5. writes the new generation's firmware folder before `switch-to-configuration boot`, so the
   bootloader packs the right early-boot firmware.

Pages holds every package each release: about 210 MB plus the new families, well inside the 1 GB
site limit.

### 6. A device plugged in later

*Settings › Updates and recovery › Firmware for a new device* runs detection for the running
system. If it names a package this laptop doesn't have, it offers to download it (online, from
the current channel, verified the same way) or to take it from a WrOS USB stick. It adds it to the
running generation's folder and the initrd, and tells you to unplug and replug the device or
restart. A new USB Wi-Fi adapter, for example, works without an update.

If the channel has moved on to a newer release, it uses that release's package: firmware files
don't depend on the kernel build, only on their names.

### 7. Installing

The live ISO has every package. The installer runs detection against the target system, copies
the packages it names into the target's store and writes the first generation's folder.

### 8. The transition from rc32

A laptop on rc32 (or earlier) updates to rc33 with its old engine, which knows nothing of
firmware sets. To keep its Wi-Fi and screen:
- rc33's system closure carries the Stream's packages (rtw88, i915-glk, sof) as plain store
  paths, not as `hardware.firmware`;
- the bootloader script and activation each run `wros-firmware ensure` when the generation has no
  folder yet: detection against that system, using packages already in the store.

On the Stream that builds the full set at the restart, from what came with the update. The seed
stays while a laptop on rc32 or earlier can update straight to the newest release.

## Safety

`wros-firmware` and the engine changes run as root. Every package comes from a path the signed
manifest names and is verified before use. The tool writes only under `/nix/var/wros/firmware`,
`/nix/var/nix/gcroots/wros-firmware` and the `/lib/firmware` link, and refuses symlinks planted
there.

## Tests

- **Unit (Python):** detection against fake `/sys` and `modules.alias` trees; the symlink folder,
  GC roots and pruning; the initrd cpio's contents; the engine's firmware section (fetch only
  what's needed, reuse what's there, refuse a bad hash).
- **NixOS VMs with declared hardware:** QEMU devices with known drivers, such as an emulated
  Intel e1000e or a USB device needing firmware, so detection picks a package, plus the kernel's
  `test_firmware` module to prove a file in `/lib/firmware` loads after boot and one in the
  appended initrd loads in stage 1. Check that WrOS's kernel builds `test_firmware`.
- **Update path:** the thin-update VM test goes rc32 → rc33, with the old engine, and keeps its
  firmware via the seed. A second hop with the new engine fetches only the detected packages.
  Go back boots the previous folder.
- **On the Stream (owner):** Wi-Fi, display and sound after the rc33 update, and after go back.

## Approaches considered

1. **A set per system generation, loaded from `/lib/firmware` and an appended initrd (chosen).**
   One path for all firmware, early and late. Go back and kernel changes work because each
   generation has its own set. It costs a small root tool and a bootloader hook.
2. **One global firmware profile with its own generations.** Simpler to write, but it drifts
   from the system: going back to a version whose kernel wants older file names would load the
   newer set, and both histories need cleaning up separately.
3. **A larger common set in every system.** No per-machine logic, but it costs every laptop
   hundreds of MB and still misses hardware. rc32 rejected it for that reason.

## Calls I made (the owner can overrule)

- Coverage: Intel and AMD graphics, the common Wi-Fi and Bluetooth chips, and SOF sound. NVIDIA
  left out.
- A device plugged in later gets firmware from a Settings row, not a pop-up on plug-in.
- Offline extras come from a WrOS USB stick.
- The Stream's seed packages stay in the system for the transition, a few MB.

## Left to the builder

- `wros-firmware`'s command-line shape and file layout.
- How the module table is stored (JSON in the system).
- The exact QEMU devices for the VM tests.
