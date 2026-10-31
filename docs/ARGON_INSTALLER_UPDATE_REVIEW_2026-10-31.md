# Installer and update contribution review — 2026-10-31

This contribution starts at development `4eeca44`. Original source is retained
in `1974923`; `ARGON_CHANGELOG.md` is the contributor's unverified report, not an
executed qualification receipt. In particular, its described Rust helper API,
test count, PID/parent behavior and service activation do not all match source.

## Implemented and repaired

The native Settings installer entry requires a live session, defaults to Cancel,
saves both buffers, refuses source conflicts and waits for backup operations.
The console reader is retired before pkexec. Only explicit root supervisor exit
codes for completed/cancelled/failed-but-cleaned installation allow a fresh input
reader; unknown cleanup status saves/rescues and exits without resuming input.
Automatic VT return also requires the original writer PID/birth identity to remain
alive and its input reader to remain retired. Parent death or a replacement reader
leaves the VT transition for explicit recovery, preventing password tail input
from reaching a newly started writer.

The root supervisor validates the live marker, read-only medium, caller binary,
UID, process birth, logind seat/TTY and retired input readers. Systemd owns its
cgroup and stop cleanup. Private mount/UTS/IPC namespaces retain the host PID
view needed to monitor the caller. The submitted private PID namespace hid that
caller and made the PID-1/parent checks impossible. An actual transient-systemd
namespace probe reproduced the old failure and confirmed the corrected parent
visibility and private mount/hostname behavior. That probe touched no disks,
accounts, terminals or packages; it is not an installer VM qualification.

Installer password entry runs under a dedicated locked `_wros-installer`
identity, not the writer's UID. Image staging creates that account only inside
the checked staging root, and the image audit refuses root/unlocked/duplicate
identities. Existing frontend core-dump/ptrace protection remains in use.

Storage ownership now requires exactly one eligible unmounted target disk, with
device number, capacity and kernel disk-sequence identity captured before the
backend starts. Mounted aliases and existing mapper ancestry are protected;
active swap or ambiguous eligible disks refuse startup. Guided disk selection
uses that checked identity without pre-approving writes. Cleanup refuses changed
device identity and never closes a new mapping merely because it is writable.
Other newly created disks/mappers remain outside its scope. Installation proof
must match a fresh session nonce, the selected disk's encrypted ancestry, current
application hashes, installed kernel/initrd/GRUB and an EFI loader.

The update backend adds dated metadata inspection, POSIX lock coordination,
durable staging/application journals and real maintained APT planning. Planning
has no host sources/downloads, permits only the authenticated bundle and refuses
removals. Real solver tests build disposable .deb files and cover missing and
reverse dependencies while preserving the installed-status fixture. Post-apply
checks require the reviewed installed versions. A foreign boot marker is left
untouched; an owned marker is retired before package execution. Failed or timed
out transactions retain archives for rescue and record failure, not success.
Real application cannot accidentally use the host database when a test supplies
custom state paths. Reboot requests belong only to an owned transaction.

## Return list: unfinished release work

1. **Complete installer VM and first boot (#3):** rebuild deliverable media with
   the matching authenticated backend/capsule and isolated frontend account.
   Exercise the actual new production handoff, user cancellation, backend and
   cleanup failure, full encrypted install, wrong/correct unlock, login and
   installed writing on a freshly created disposable QEMU disk. Current synthetic
   storage/UI tests and earlier QA-launcher VMs do not establish those results.
   Multi-target selection and active-swap handling remain deliberately refused.
2. **Update apply/interruption/rescue (#5):** native root mutations remain refused;
   the offline unit is **not enabled** in image staging. Complete the native
   review/stage/cancel/repair workflow, exact signed metadata retention and
   boot-time revalidation/freshness policy, crash-consistent publication and
   interruption/rescue VM cases before activating it. POSIX preflight checks do
   not replace APT's own transaction locks. ext4 has no automatic snapshot
   rollback; btrfs directories alone do not qualify bootable rollback either.
3. **Live/provider/device acceptance:** real Wi-Fi, Google credential protection
   and provider restoration, lock-before-sleep, hardware/power-loss, release media
   and corresponding-source/notices gates remain in the prior integration list.

Return to these gaps in the stated order. Do not mark either installer first boot
or the update engine production-qualified from source inspection or mocked APT.

## Qualification

The [complete local software run](evidence/installer-update-local.json) passed
1,101 Rust tests per overlapping configuration (five/four explicitly ignored),
240 Python tests with zero skips, both strict Clippys and 21 gates. The
[independent CI receipt](evidence/installer-update-ci.json) passed the same test
counts and both Clippys across 20 gates; the extra local gate audits the staged
Wi-Fi payload. [CI provenance](evidence/installer-update-ci-provenance.json)
binds run 41230828478, code head `0c48457` and its PR test merge to the same 492-file
source SHA-256 `167b25c278108a9c09883c8bdc11b76dce2e17ab6c9bbdcef53c78c0b5bf1d5b`.
The following documentation/evidence retention commit changes no qualified source
or verifier inputs. [Local provenance and four-size backup regression frames](evidence/installer-update-local-provenance.json)
retain the executed binary and original input hashes; those frames are not full
installer VM evidence. Historical named evidence remains intact. No host
installation disk, real Google account or manuscript was used.
