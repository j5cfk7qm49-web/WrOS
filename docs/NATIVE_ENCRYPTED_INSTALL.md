# Native encrypted installation qualification

Draft PR 51 adds an explicit installation mode to the existing guarded native
installer fixture. It creates one new marked 16 GiB virtual target, uses no NIC
or host disk, and answers only recognized questions through the shipped Grimoire
frontend. Disk-write questions remain default No; the fixture explicitly selects
Yes only after checking the generated target's identity. Unknown forms stop the
test without an answer.

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

Completion requires more than a successful backend exit. A read-only in-target
audit checks the generated account, locked root, native authentication, installed
encryption packages, LUKS UUID, encrypted-root initrd, EFI loaders and application
bytes. The supervisor then checks that target mounts retired. A separate boot
must reject a wrong unlock key, unlock with the generated passphrase, reject a
wrong login, and reach the installed non-root Grimoire session. These assertions
are implemented; a full passing encrypted installation and boot are still pending.

Production handoff, completion UI, owned post-write LUKS/LVM cleanup, embedding
the capsule in deliverable media, Secure Boot and physical qualification remain
open. The root helper here remains restricted to marked disposable QEMU guests.
