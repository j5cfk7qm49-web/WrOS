# Native encrypted installation qualification

Draft PR 51 adds an explicit installation mode to the existing guarded native
installer fixture. It creates one new marked 16 GiB virtual target, uses no NIC
or host disk, and answers only recognized questions through the shipped Grimoire
frontend. Disk-write questions remain default No; the fixture explicitly selects
Yes only after checking the generated target's identity. Unknown forms stop the
test without an answer.

Fourteen focused identity, credential-policy and encrypted-root ancestry checks
pass. Earlier [fixture CI](evidence/native-encrypted-install-before-adapter-ci.json)
and [passphrase-adapter CI](evidence/native-encrypted-install-before-overlay-ci.json)
each passed 973 Rust tests per configuration and 168 Python tests with zero
skips. Their original source identities remain separate. The overlay revision's
full CI and new installation/boot test are pending.
The first current-source CI ran all 973 Rust tests per configuration and 169
Python tests with zero skips, then [failed its last whitespace gate](evidence/native-encrypted-install-capture-whitespace-provenance.json)
on a trailing terminal cell in a generated journal capture. Capture serialization
now omits that padding after validating the full frame. Both real demo captures,
documentation and `git diff --check` pass locally; the new full CI remains pending.

Two independent generated fixture credentials stay private. The account uses a
generated SHA-512 crypt hash in the test preseed. The encryption passphrase is
typed into hidden native fields and never supplied through preseed or arguments.
Shipping installer defaults do not approve disk writes or contain credentials.

The [first actual attempt](evidence/native-encrypted-install-first-failure.json)
stopped at Debian's missing `cdebconf-passthrough-entropy` package request.
`partman-crypto` 134 requests this frontend UI helper even for passphrases. The
fixed-version [adapter](../os/config/installer/native-crypto-passphrase) accepts
only the authenticated Debian script's exact hash and removes that unused
request for the native passphrase path. Maintained `blockdev-keygen`, cryptsetup
and kernel randomness remain unchanged. No entropy-plugin capability is
advertised; Debian continues to reject unsupported random-key requests.
The stock text frontend retains its maintained entropy plugin.

The [second attempt](evidence/native-encrypted-install-media-fault.json) reached
both hidden passphrase fields and copied the system, then remained at
[APT source configuration](evidence/native-encrypted-install-apt-stage.png).
It was explicitly stopped after a source audit found that Debian's subsequent
nonrecursive CD bind drops the previous private view's submounts. That attempt
has no installation/boot pass. The new view uses one overlay filesystem with
the read-only live ISO below a private RAM upper layer containing the verified
capsule and native layout. It checks
an actual nonrecursive bind of that filesystem before starting the frontend.
Bounded periodic backend diagnostics now accompany long fixture transitions.
A [pre-frontend remount attempt](evidence/native-encrypted-install-remount-failure.json)
failed without changing any target byte. The overlay is now sealed through the
existing VFS read-only bind operation rather than a filesystem remount.
The subsequent [prefix attempt](evidence/native-encrypted-install-prefix-failure.json)
passed the bind check and reached the native menus, then Debian reported a
missing `native-installer/pool/...` path. The target again stayed unchanged and
the retry question received no answer. The capsule now keeps that exact prefix;
admission verifies every indexed archive's path and bytes after the bind.

The [next full-copy attempt](evidence/native-encrypted-install-apt-failure.json)
passed that admission and copied the system, then failed in package-manager
configuration. Debian loaded the mirror provider while its CD provider was only
available in the pool. Capsule authentication catalogs also appeared as flat
repositories to `apt-cdrom`, and queued `dosfstools` lacked an offline archive.
The next backend explicitly includes maintained `apt-cdrom-setup`; the media view
exposes only the capsule archive subtree and preserves its external mount through
Debian's hybrid-media interface. The refreshed image requests and audits
`dosfstools` in both the installed runtime and binary pool. Builds and full
installation/boot qualification for these composition changes are pending.

Completion requires more than a successful backend exit. A read-only in-target
audit checks the generated account, locked root, native authentication, installed
encryption packages, LUKS UUID, encrypted-root initrd, EFI loaders and application
bytes. The actual mounted root must have exactly the fixture's LVM-to-LUKS2-to-
target-partition ancestry. The supervisor then checks that target mounts retired. A separate boot
must reject a wrong unlock key, unlock with the generated passphrase, reject a
wrong login, and reach the installed non-root Grimoire session. These assertions
are implemented; a full passing encrypted installation and boot are still pending.

Production handoff, completion UI, owned post-write LUKS/LVM cleanup, embedding
the capsule in deliverable media, Secure Boot and physical qualification remain
open. The root helper here remains restricted to marked disposable QEMU guests.
