# Linux foundation decision

Decision date: 2026-10-24. **Selected: Debian 13 Stable (trixie), initially amd64.** No custom kernel. Other architectures require their own image and boot tests.

## Comparison

The tradeoff assessments below are engineering judgments, not measured battery/boot benchmarks. A minimal configuration matters more than the distribution's default desktop footprint.

| Foundation | Strength for this appliance | Cost / risk | Decision |
|---|---|---|---|
| Debian Stable, minimal systemd installation | Conservative packages, familiar Debian installer and apt security maintenance; straightforward NetworkManager/logind/polkit and console-setup; glibc compatibility for Rust and optional Python tools | Older application versions; rollback is not atomic by default; very new hardware may need firmware/backports | **Choose**; smallest integration delta for this repository and available build environment |
| Fedora minimal / custom Atomic appliance | Maintained kernel/drivers and strong systemd integration; Atomic deployments offer bootable previous system state | More frequent major upgrades; custom no-desktop image composition and release maintenance; desktop Atomic images include an unnecessary desktop | Viable alternative if automatic OS deployment rollback dominates requirements |
| Ubuntu LTS minimal | Similar Debian ecosystem; mature installer/hardware ecosystem | Additional distribution-specific packaging decisions; no demonstrated advantage for this prototype | Reasonable supported development host; not the first appliance target |
| Alpine minimal | Small package/service footprint | musl/Python binary-wheel compatibility adds integration work for spaCy and optional writing tools; different service assumptions for the chosen Linux interfaces | Do not optimize base size at the cost of the offline toolchain and safety integration |
| NixOS appliance configuration | Declarative rebuilds and generation rollback are attractive | Different packaging/service model and contributor skill requirements; offline closure management needs deliberate tooling | Reconsider only with a dedicated maintainer and reproducible offline-update proof |

Debian's official release information identifies 13.7 as the current point release, with regular support through 2028-08-09 and LTS through 2030-06-30; coverage and architectures narrow under LTS. These dates are not a guarantee that every optional package is security-supported. [1](https://www.debian.org/releases/trixie/)

Fedora Silverblue's official page describes a roughly 13-month lifecycle and a GNOME desktop; the official Atomic documentation describes rebase and rollback of deployments. A custom appliance would need its own no-desktop composition, rather than shipping Silverblue unchanged. [3](https://fedoraproject.org/atomic-desktops/silverblue/) [4](https://docs.fedoraproject.org/sq/atomic-desktops/updates-upgrades-rollbacks/)

## Concrete package evaluation

The development host and built guest are Debian 13.7. The recipe uses signed `trixie`, `trixie-security` and `trixie-updates` repositories. Actual versions are recorded by the builder, not guessed from the distribution name. Relevant available packages include NetworkManager, polkit, systemd, kbd 2.7.1, console-setup, xkb-data, UPower, brightnessctl, power-profiles-daemon, cryptsetup, rclone, WordNet, proselint, eSpeak and Pandoc. Only the minimal package list in `os/config/packages.txt` is in the current VM; optional analyzers/export/reference/cloud tools are not silently advertised as bundled.

**Keyboard correction from practical testing:** upstream kbd supports modern Colemak-DH, but Debian's installed `kbd`/`console-data` packages did not supply named Colemak maps at the expected paths. The tested recipe compiles the maintained XKB `us` variants with `ckbcomp` instead. `loadkeys --unicode --bkeymap` validates them without opening a console; `loadkeys --parse` alone was not usable in this container. This avoids vendoring an unverified keymap and includes the Unicode/modifier tables. The resulting DH physical E → logical F mapping and timer rollback were verified in the VM.

## Image and reproducibility strategy

`mmdebstrap` creates a staged Debian rootfs. Regular FAT/ext4 files plus `sgdisk`, mtools and `grub-mkstandalone` create a GPT/UEFI VM disk without host loop mounts or block-device access. The kernel/initramfs are Debian packages. QEMU/OVMF boots the complete disk, rather than only invoking an application in a container.

This is a **reproducible workflow**, not yet a bit-for-bit reproducible release. Default security repositories evolve; package resolution, filesystem timestamps/UUID metadata and build toolchain must be pinned/normalized for identical bytes. A reviewed snapshot mirror can be passed using `DEBIAN_MIRROR`; actual package inventory and image checksum are always emitted. Snapshot date/valid-until handling, apt source export and signed image release metadata remain release gates. Do not disable package signature verification to obtain reproducibility.

The current firmware image is unsigned and requires Secure Boot disabled in the VM. A physical installation should retain Debian's supported signed shim/GRUB/kernel chain until a separately tested WritingDeck signing policy exists.
