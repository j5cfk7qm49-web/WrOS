# Native installer backend integration

This increment assembles an authenticated offline package pool and stages it
with the matching installer initrd as regular files. It does not grant a root
launcher or certify installation from the live desk. The delivered live image
and shared Grimoire frontend remain separately qualified pieces.

`tools/build-live-installer-pool.py` uses maintained python3-debian and
python3-apt for Debian metadata, versions, virtual providers and dependency
syntax. GPG verifies the original Debian main/security InRelease files. The
exact authenticated package-index bytes are parsed from a sealed snapshot;
changed inputs cannot subsequently be copied into the pool or APT state.
Each archive's size, SHA-256, control identity and dependency fields are checked.
Missing archives can be fetched by APT with private state and explicit
`--download`; the default requires existing verified archives.

The [actual offline build](evidence/native-pool-build.json) contains **401
packages / 163,256,088 bytes**, including **92 kernel components** for live
`6.12.113+deb13-amd64` and the separate `6.12.107+deb13-amd64` installer.
Native/text, partitioning, crypto, EFI bootloader, account, finish and recovery
components are mandatory. The manifest records 25 optional components omitted
because their installer-package dependencies are unavailable. No dependency is
satisfied by inventing a provider or borrowing a package from the builder.

Cdebconf stays pinned at 0.280 for the qualified transport ABI. Every archive
payload is checked to prevent a later unpack from replacing the native
fail-closed passthrough module. The [focused verification](evidence/native-pool-local.json)
passed nine non-root offline fixture tests, including actual Debian archives,
epoch/virtual dependency cases, signature-policy boundaries, changed inputs,
path escape and module collision. GPG is mocked only in policy unit tests;
the actual build separately verified real Debian signatures and archive hashes.
The generated package index was identical across independent output builds.

Run on a Debian 13 amd64 builder with `python3-debian`, `python3-apt`, `gpgv`,
`debian-archive-keyring`, `dpkg-dev` and already authenticated APT lists:

```sh
python3 tools/build-live-installer-pool.py \
  --indexes /path/to/apt.udeb/state/lists \
  --archive-root /path/to/live-build/binary \
  --archive-root /path/to/verified/kernel-udebs \
  --output /path/to/new-native-pool
```

The output is intended for `native-installer/` on the medium. Its generated
subset index is WrOS metadata; it is **not a Debian-signed Release**. The
original signed metadata and complete archive hashes are retained separately.
The current live ISO has not been rebuilt with this pool.

`tools/stage-native-installer.py` independently revalidates that pool from its
retained signed Debian indexes and every archive, rather than trusting its build
receipt. It checks the native transport inside the compressed backend, rejects
extra kernel module versions, and requires the backend kernel to equal the
actual live medium's kernel bytes. The output includes the backend, pool,
original signatures, licenses and file hashes. Generated media metadata says
WrOS and preserves the ordinary binary-package index checksums; it makes no
claim to be a Debian signature. Existing outputs and symlinks are preserved.

The eighteen focused tests cover the package pool, real CPIO capsule fixtures
and the diskless guest guard. Synthetic kernel/module bytes are confined to
unit fixtures; actual staging uses the separately qualified Debian image and
the pinned transport hash. This distinction is retained in the test sources.

The [actual staged capsule](evidence/native-capsule-build.json) and
[eighteen focused checks](evidence/native-engine-focused.json) passed. The
[diskless no-NIC guest](evidence/native-engine-menu-vm.json) then passed in
**122.03 seconds**: Debian's maintained startup and menu presented actionable
language and location forms through the actual non-root WrOS frontend on tty7.
Actual [language](evidence/native-engine-language.png) and
[location](evidence/native-engine-location.png) captures show the shared
Grimoire palette and panel code, with their [pixel identities](evidence/native-engine-captures.json).

This is a guarded VM fixture with a separate read-only QA medium, not a root
grant installed in the live ISO. It rejects writable media, verifies its QEMU
marker and capsule bytes, and retires the previous editor/input reader before
starting the TUI. A standard temporary systemd service contains the backend
and frontend; private mount/PID/UTS/IPC namespaces keep the backend's media,
boot arguments and hostname changes separate. Stopping that service retired
both processes and removed the private backend mounts; the guest powered off.

Debian's live plugin prefers a GTK initrd even in text mode. The native private
view excludes that path and carries all verified overmounts recursively, with
byte checks on the exact initrd and package-index paths Debian reads. Its
separate host kernel check uses noninteractive debconf; later installer forms
use the independent native stream. The backend's inherited descriptor avoids
debconf's reserved low descriptors. Original upstream files and copyrights are
retained in the fixture payload. [Earlier failed observations](evidence/native-engine-attempts.json)
keep their original sources and results.

The next integration must package this path into the live image with a trusted
editor handoff and complete interruption handling. Disk review, LUKS state
cleanup, fresh target installation/reboot, firmware boot and physical hardware
checks remain open. The diskless fixture proves startup, native form exchange
and service-stop containment; it has no target disk and cannot qualify an
installation. No host disk or account was changed.

The [full local software run](evidence/native-engine-local.json) passed 973
Rust/configuration, 153 Python/zero skips, both Clippys and 21 gates. Its
[source and binary provenance](evidence/native-engine-local-provenance.json)
matches the focused and guest reports. Debian CI remains pending; this local
record does not imply a remote result or target installation acceptance.
