# Debian-first component reuse

The implementation rule is **reuse maintained Debian components first**. Where
Debian lacks a suitable version, use a pinned, reproducibly built upstream FOSS
release with its source and notices retained. Write WrOS-specific interface and
integration code where needed. Check existing components before adding a new
engine, privileged protocol, storage format or daemon.

The user explicitly authorized this approach for remaining work and existing
parts that can be strengthened. It does not change Grimoire manuscript formats,
require a graphical desktop, enroll accounts or upload manuscripts. Ordinary
management remains in the native WrOS interface. Preserve working behavior and
tests when adopting a component; package availability alone is not integration
or hardware qualification.

**Interface policy: TUI-first everywhere practical.** Use the text-mode Debian
Installer, native text login/settings, text update/recovery/backup flows and the
existing Grimoire writing UI. Backend daemons, libraries and headless converters
are compatible with this policy. Do not add a graphical desktop or launch GUI
frontends as the ordinary workflow. Keep optional provider-browser requirements
explicit when a service cannot authorize entirely through a text interface.

**One Grimoire interface:** the live installer and every native WrOS surface
must share Grimoire's themes, widgets, layout/spacing, focus and selection,
navigation, shortcuts and help/status patterns. Reuse the existing UI/design
code. Backend adoption must appear through that common interface. Installation,
updates, recovery, backup and account management are parts of the same product;
upstream frontends are not the ordinary user-facing workflow. Keep advanced
emergency maintenance explicit and separate from normal product navigation.

The desired installer is a **live WrOS session with its own installer TUI over
Debian's installation backend**. Reuse supported preseeding/debconf interfaces
and the maintained live-installer environment; keep disk/encryption/account and
bootloader operations in Debian components. The live launcher has a text path
and requires a matching live/installer kernel. Verify that boundary before
shipping the frontend. Keep secrets and reviewed disk answers per-run in private
RAM storage, with no unattended destructive answers in the shared ISO defaults.

## Decisions

| Area | Reuse | WrOS work / decision |
|---|---|---|
| Boot and installation | Debian live-build, Debian Installer/live-installer and its text launcher path, cryptsetup, initramfs-tools, signed Debian shim/GRUB/kernel | Add the native installer TUI in a live WrOS session over the existing backend. Qualify kernel matching, LUKS, rescue, interruption and Secure Boot in new guests. Retain a standard text recovery/fallback path. |
| Login | greetd, PAM, logind; evaluate packaged tuigreet | Retain qualified native account/layout recovery and the fixed writing-session command. Consolidate UI only if the packaged greeter preserves those requirements. No WrOS password verifier. |
| Lock and sleep | vlock/PAM and systemd-managed lifecycle | Replace the experimental custom request/response service protocol with standard systemd execution and status handling. Keep reader retirement, display/query pause and real authenticated completion checks. Suspend remains disabled until the complete protocol and resume are qualified. |
| Network enrollment | NetworkManager, libnm / its maintained introspection bindings | Use its connection, authorization and secret-agent APIs. Add the native selection/credential interface; no Wi-Fi protocol implementation or separate password store. |
| Battery and power | UPower, power-profiles-daemon, logind | Prefer UPower's composite display device to hand-maintained battery aggregation. Retain safe missing-device behavior and explicit power/save guards. |
| Brightness and warmth | brightnessctl; evaluate gammastep/Redshift DRM backends | Keep brightnessctl. Test packaged gamma backends before expanding the DRM bridge. Preserve original calibration, output selection, independent preview rollback and unsupported-driver refusal. |
| Audio and speech | PipeWire, WirePlumber, ALSA; Speech Dispatcher and eSpeak NG | Use maintained volume/device and speech engines. Add native controls, a scene queue and source-linked speech callbacks. No custom audio server or speech synthesizer. |
| Clock and timezone | timedated, systemd-timesyncd | Keep the qualified timezone integration; extend the same maintained service for clock/NTP controls. Preserve historical writing dates. |
| Accounts and credentials | AccountsService, PAM, libsecret / a maintained Secret Service provider | Native account and optional credential management on existing services. Preserve normal unprivileged writing and authenticated administration. |
| Storage | UDisks2, cryptsetup, standard filesystem tools | Native device/status/mount workflows through maintained APIs. Retain explicit target review and wrong-disk protections. |
| Updates | APT, PackageKit and systemd offline-update support | Use signed Debian metadata and an existing transaction engine. Prefer prepared offline updates with native save/restart confirmation. Select one apply policy; do not build a package manager or run competing unattended apply flows. unattended-upgrades remains an alternative policy component. |
| Recovery and rollback | Debian Installer rescue; Snapper and Btrfs for a qualified snapshot layout | Keep root snapshots separate from manuscript storage and backups. Snapper rollback requires a properly configured Btrfs root; the existing ext4 image does not gain rollback merely by installing Snapper. No automatic conversion of existing installations. |
| Versioned backup | Restic; Rclone for optional remote adapters | Use existing repositories, snapshot, integrity and restore engines. Add destination selection and verified restore to a new directory. Google enrollment and transmission remain explicit opt-in actions. |
| Publishing | Pandoc, optional headless LibreOffice, Poppler and packaged fonts | Reuse DOCX conversion and PDF rendering. Test actual OOXML/page footnotes and original preservation; retain existing exporters until replacements pass. Typst is an optional upstream-build alternative, not required for the Debian-only path. |
| Memory and device health | systemd-zram-generator; smartmontools / fwupd where appropriate | Evaluate a small zram profile and native read-only health/status first. Use one compression policy. Physical firmware operations and battery behavior require identified hardware. |
| Worker isolation | bubblewrap and ordinary process/resource limits | Evaluate the packaged sandbox for document/converter workers rather than inventing a sandbox. Preserve offline operation, cancellation and explicit file access. |
| Fiction workflow | Existing Grimoire, established Rust libraries and existing text/history machinery | Continuity facts, scene knowledge, annotation interaction, version/variant UI and tutorial verification remain WrOS-specific. Preserve Markdown/YAML and the tested authoring workflow. Reuse libraries and converters underneath them. |

Package versions and availability are an observed Debian 13 metadata snapshot,
not a promise to freeze security updates. Record actual versions and hashes for
each build. Do not install every alternative or optional publishing/health pack
into the base image merely because it is listed here.
The [machine-readable inventory](foss-reuse.json) records 63 candidates, with
60 available in the configured Debian 13 metadata. `grub-btrfs`, `kmscon` and
`typst` were absent there; upstream builds remain available if a selected
workflow actually needs them.

## Important fit checks

### Locking

Debian's `physlock` 13-1.1 was inspected as a possible consolidation. Its
SIGTERM/SIGQUIT handler calls `exit(0)` while still locked, and its locked cleanup
path leaves VT release to recovery. Exit code zero therefore cannot serve as
WrOS's authenticated-unlock acknowledgment. The inspected Debian ELF also has
no libsystemd dependency; the source's optional logind code is conditional on a
C macro that the inspected build does not define. This matters because greetd's
actual tty descriptor and session identity must not be inferred from device-node
ownership or utmp alone. These are concrete fit gaps, not a claim that the
package cannot be used in other deployments.

Keep packaged vlock's authentication and signal-failure behavior, and use
maintained systemd lifecycle primitives for ownership outside the user's session
cgroup. The larger unqualified request/path-service prototype is preserved only
on local branch `codex/lock-service-prototype` (`6014e92`) for comparison. It is
not a selected architecture or qualified release component.

### Recovery

Snapper documents that rollback requires Btrfs and proper boot configuration.
The current image uses a different layout. A new rollback-enabled profile needs
real install/update/rollback tests, retained encrypted recovery access, and a
separate manuscript data boundary. RAUC and OSTree are available alternative
update architectures; do not bolt multiple incompatible root-update models onto
the existing APT system.

### Remote backup and conversion

Rclone already implements Google Drive authorization and conversion, but its
current documentation requires an appropriate client registration; do not
assume an old shared client ID remains usable. Keep live provider qualification
separate from local fixture tests. Conversion can be lossy, so use explicit
materialization and verified round trips for external document formats.

## Adoption order

1. Simplify lock lifecycle around packaged vlock and systemd, then pass actual
   wrong/right PAM, reader/VT ownership, repeated locking and process-death tests.
2. Strengthen existing system settings with UPower, PipeWire/WirePlumber and
   NetworkManager APIs, and integrate the selected update transaction service.
3. Integrate Restic restore, packaged speech, and the Pandoc/rendering pipeline
   through the native interface with real local fixtures.
4. Qualify the chosen encrypted/recovery/rollback installation profile and build
   a refreshed combined image. Retain historical image and failed-test evidence.
5. Complete the remaining fiction-specific interaction/tutorial work and the
   physical qualification that cannot be established by package installation.

## Primary references

- [Debian physlock package](https://packages.debian.org/trixie/utils/physlock)
  and [upstream source](https://github.com/xyb3rt/physlock).
- [systemd-run lifecycle](https://manpages.debian.org/trixie/systemd/systemd-run.1.en.html).
- [UPower composite display device](https://upower.freedesktop.org/docs/UPower.html).
- [PackageKit offline updates](https://www.freedesktop.org/software/PackageKit/doc/offline-updates.html).
- [Snapper snapshot and rollback contract](https://snapper.io/manpages/snapper.html).
- [Restic restore](https://restic.readthedocs.io/en/stable/050_restore.html).
- [Rclone Drive](https://rclone.org/drive/).
- [Pandoc formats and footnotes](https://pandoc.org/MANUAL.html).
- [Debian Live installer integration](https://live-team.pages.debian.net/live-manual/html/live-manual/customizing-installer.en.html)
  and [debconf passthrough frontend](https://sources.debian.org/src/debconf/1.5.91/doc/passthrough.txt/).
