# Maintained component reuse

Reuse maintained Nixpkgs components first. Where they do not provide the required
behavior, use a pinned upstream FOSS release with its source and notices retained.
Keep WrOS-specific interface and integration code small, and check existing
components before adding an engine, privileged protocol, storage format or daemon.

The user explicitly authorized this approach for remaining work and existing
parts that can be strengthened. It does not change Grimoire manuscript formats,
require a graphical desktop, enroll accounts or upload manuscripts. Ordinary
management remains in the native WrOS interface. Preserve working behavior and
tests when adopting a component; package availability alone is not integration
or hardware qualification.

**Interface policy: one native WrOS interface.** Graphical unlock, loading, login
and writing are the defaults, with text fallbacks and explicit console recovery.
Installation, settings, updates and backups share the native interface. Do not
add a desktop or use upstream GUI frontends as the ordinary workflow. Keep
provider-browser requirements explicit.

**One Grimoire interface:** the live installer and every native WrOS surface
must share Grimoire's themes, widgets, layout/spacing, focus and selection,
navigation, shortcuts and help/status patterns. Reuse the existing UI/design
code. Backend adoption must appear through that common interface. Installation,
updates, recovery, backup and account management are parts of the same product;
upstream frontends are not the ordinary user-facing workflow. Keep advanced
emergency maintenance explicit and separate from normal product navigation.

The live WrOS session launches the native installer through a supervised session
and backend. `nixos/scripts/wros-install-core` uses standard GPT, cryptsetup and
filesystem tools, installs the prebuilt NixOS closure offline, creates the writer
account and installs GRUB last. It rechecks disk identity and use before writing.
Secrets travel over private stdin and remain per-run.

The Debian Installer/preseeding backend and matching live/installer kernel choice
belong to the earlier Debian builds.

## Decisions

**Debian-era component evaluation and proposals.** Candidate availability does
not mean installation. NixOS uses the native `wros-greeter`, PAM, implemented
clock/NTP and sound controls, and the packages/services declared in
`nixos/modules/`. The boot/installation, lock/sleep, update, recovery and backup
rows below describe current mechanisms; the other rows retain earlier proposals.

| Area | Reuse | WrOS work / decision |
|---|---|---|
| Boot and installation | NixOS live ISO, standard GPT/cryptsetup/filesystem tools and GRUB | The supervised native installer rechecks disk identity and use, installs the prebuilt closure offline, creates the writer account and installs GRUB last. Secrets use private per-run stdin. Qualify physical installation and recovery separately. |
| Login | greetd, PAM, logind; evaluate packaged tuigreet | Retain qualified native account/layout recovery and the fixed writing-session command. Consolidate UI only if the packaged greeter preserves those requirements. No WrOS password verifier. |
| Lock and sleep | vlock/PAM and systemd-managed lifecycle | Guarded lock-and-suspend is implemented; direct suspend requests remain refused. Physical wake, repeated handoff and display/input behavior require device-specific qualification. |
| Network enrollment | NetworkManager, libnm / its maintained introspection bindings | Use its connection, authorization and secret-agent APIs. Add the native selection/credential interface; no Wi-Fi protocol implementation or separate password store. |
| Battery and power | UPower, power-profiles-daemon, logind | Prefer UPower's composite display device to hand-maintained battery aggregation. Retain safe missing-device behavior and explicit power/save guards. |
| Brightness and warmth | brightnessctl; evaluate gammastep/Redshift DRM backends | Keep brightnessctl. Test packaged gamma backends before expanding the DRM bridge. Preserve original calibration, output selection, independent preview rollback and unsupported-driver refusal. |
| Audio and speech | PipeWire, WirePlumber, ALSA; Speech Dispatcher and eSpeak NG | Use maintained volume/device and speech engines. Add native controls, a scene queue and source-linked speech callbacks. No custom audio server or speech synthesizer. |
| Clock and timezone | timedated, systemd-timesyncd | Keep the qualified timezone integration; extend the same maintained service for clock/NTP controls. Preserve historical writing dates. |
| Accounts and credentials | AccountsService, PAM, libsecret / a maintained Secret Service provider | Native account and optional credential management on existing services. Preserve normal unprivileged writing and authenticated administration. |
| Storage | UDisks2, cryptsetup, standard filesystem tools | Native device/status/mount workflows through maintained APIs. Retain explicit target review and wrong-disk protections. |
| Updates | Nix binary-cache import, signed WrOS closure metadata and systemd offline-update support | Review and stage a signed bundle in Settings, then switch the system generation at restart. Keep the previous generation for go-back. |
| Recovery and rollback | Authenticated WrOS recovery specialisation and NixOS system generations | Go-back selects the previous system generation without Btrfs or Snapper. Repair checks Nix store integrity and reactivates the current boot generation. Manuscript backups remain separate. |
| Versioned backup | Restic; Rclone for optional remote adapters | Native local/Drive destination setup, masked second-device authorization, private recovery keys, encrypted snapshots and actual restore/hash checks now use these engines. Configured projects can back up automatically after a pause; a Drive repository uploads encrypted data. Automatic snapshots have retention pruning, while manual snapshots are retained. The helper stages saved files rather than mounting the active manuscript; live Google qualification remains open. See [native backup](NATIVE_BACKUP.md). |
| Publishing | Existing native DOCX writer, pulldown-cmark, optional headless LibreOffice, Poppler and packaged fonts; Pandoc for additional formats when needed | Manuscript page notes reuse the pinned parser, DOCX package and LibreOffice conversion. Real page fixtures use Poppler/Liberation; EPUB notes reuse its maintained HTML renderer and Debian EPUBCheck; paperback notes are real Word footnotes and LibreOffice page notes in the print document. No competing PDF typesetter. Typst remains an optional alternative. |
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

Recovery uses the authenticated WrOS recovery specialisation. Go-back selects
the previous NixOS system generation; it does not require Btrfs or Snapper. Repair
checks Nix store integrity and reactivates the current boot generation. Manuscript
backups remain separate.

### Debian-era recovery alternatives

The earlier evaluation compared Snapper, which requires Btrfs and proper boot
configuration for rollback, with RAUC and OSTree. The Debian image used ext4;
installing Snapper alone did not add rollback. These were alternative proposals
for the earlier APT system.

### Remote backup and conversion

Rclone already implements Google Drive authorization and conversion, but its
current documentation requires an appropriate client registration; do not
assume an old shared client ID remains usable. Keep live provider qualification
separate from local fixture tests. Conversion can be lossy, so use explicit
materialization and verified round trips for external document formats.

## Historical Debian adoption order

These steps record the earlier integration plan. Current mechanisms are declared
in `nixos/modules/` and `nixos/pkgs/`; use
[NIXOS_QUALIFICATION](NIXOS_QUALIFICATION.md) and the
[hardware checklist](HARDWARE_CHECKLIST.md) for candidate acceptance.

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


The contextual footnote reader reuses the locked pulldown-cmark parser and
Ratatui 0.30.2 paragraph wrapping. Its rendered-line-info feature supplies the
same line count for scrolling as the maintained renderer, avoiding a competing
word-wrap algorithm. Shared wros-shell panels retain Grimoire themes and help.
This integration does not substitute a custom speech, history or publishing engine.
