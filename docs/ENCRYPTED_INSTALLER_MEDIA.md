# Offline encrypted-target media

The [corrected live image](evidence/encrypted-media-image-build.json) passes its
actual package, payload and authentication-policy audit. All six encryption
packages are installed in the copied filesystem and have matching offline Debian
archives: cryptsetup, cryptsetup-bin, cryptsetup-initramfs, dmsetup, lvm2 and
systemd-cryptsetup. Their required runtime files match the archives byte for byte.

The [first image failed](evidence/encrypted-media-first-build.json): installing
systemd-cryptsetup in the root did not make live-build put its archive on the
medium. The recipe now supplies one explicit package list to both maintained
chroot and binary package stages. Debian APT downloads the binary-list dependency
set with an empty status database, and live-build generates the package indexes.
The failed observation keeps its original source and image identity.

[Debian CI 40780133313](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40780133313)
passed **973 Rust/configuration, 165 Python/zero skips, both Clippys and 20 gates**.
Its [artifact provenance](evidence/encrypted-media-ci-provenance.json) matches the
image and guest source, `d4c4772d...7a0d7c` / 428 files. The earlier full local
checkpoint has its own before-pool-list identity. [Eleven focused checks](evidence/encrypted-media-focused.json)
cover installed states, missing or altered hooks/binaries, archive versions and
bytes, misleading filenames, symlink refusal and the retained firmware policy.

The [actual guest reports](evidence/encrypted-media-guests.json) passed:

- Automatic live entry, native Home/power settings and shutdown: **100.79 s**.
- Authenticated manual live entry: **126.46 s**.
- Authenticated fallback without the automatic-entry marker: **116.37 s**.
- The ISO's actual native frontend reached Debian's LVM write question, selected
  No, returned to partitioning and left the entire fresh **16 GiB** disk unchanged:
  **484.97 s**. Service stop retired the frontend, backend and private mounts.

These are no-NIC guests with no host disks. The live-role guests have no writable
disk; the refusal fixture creates its own nonce-identified virtual target. It
uses the frontend already present in the ISO, with no executable override. The
[native Home](evidence/encrypted-media-home.png), [write question](evidence/encrypted-media-review.png)
and [returned overview](evidence/encrypted-media-declined.png) are actual captures
with [pixel identities](evidence/encrypted-media-captures.json).

The [new backend capsule](evidence/encrypted-media-capsule.json) was independently
revalidated against this image's exact live kernel and media metadata. It retains
the maintained Debian backend, pinned native module, 401 verified offline udebs,
original signatures and notices. The root launcher remains a separate QA fixture;
the capsule is not yet a production installer grant inside the image.

The image is **1,248,460,800 bytes**, SHA-256
`bd32aca07ec6d7cb60bccdaf691caaabbe78c4e14be03bf2217e4983bed2ccbd`.
Application inputs were preserved separately from later compiler output. This
qualifies image composition, live roles and disk-write refusal. Full encrypted
target installation/authenticated boot, post-write LUKS/LVM cleanup, the trusted
production handoff, completion UI and physical qualification remain open.
