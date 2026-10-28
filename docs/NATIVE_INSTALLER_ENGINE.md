# Native installer backend integration

The currently qualified part of this increment is an offline package pool.
It does not grant a root launcher or certify installation from the live desk.
The live image and shared Grimoire frontend remain separate qualified pieces.

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

Next integration must preserve Debian's live preparation, retriever, partman,
crypto, account and finish engines while forcing the native frontend over its
private inherited stream. Privileged lifecycle, disk review, cancellation and
interruption, LUKS state cleanup, fresh installation/reboot, firmware boot and
physical hardware checks remain open. No host disk or account was changed by
this regular-file pool build.
