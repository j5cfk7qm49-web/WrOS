# RC release contract (parallel workstreams)

Frozen interfaces and decisions for the release-candidate push. Workstreams own
disjoint files; change an interface here first, in the same commit as the code.
Prompt: ship a bootable installable ISO plus an authenticated update and rescue path.

## Decisions (reversible, recorded so every workstream agrees)

1. **Firmware**: RC requires UEFI x86-64 with Secure Boot **disabled**. Legacy BIOS is
   unsupported. `--uefi-secure-boot enable` is a build flag, not a tested claim; do not
   advertise Secure Boot until a VM boots the signed chain.
2. **Update trust**: updates verify only against `/usr/share/keyrings/wros-archive-keyring.gpg`
   *and* a pinned primary-key fingerprint list `/usr/share/wros/update-signers` (one 40-hex
   fingerprint per line, `#` comments). The Debian archive keyring is **never** consulted for
   WrOS update bundles. Both files are image inputs supplied at build time
   (`WROS_UPDATE_KEYRING`, `WROS_UPDATE_SIGNERS`); the build fails if absent. Private keys never
   enter the repository or the image. For this local RC the maintainers generate an RC-local key
   held outside the repo; a production signing identity is an outstanding release step. Tests use
   throwaway keys and the audit refuses any fixture/test fingerprint in production media.
   From rc2 the `wros` package also owns `/usr/share/wros/update-keyring.gpg`: the pinned
   primary plus the CI signing subkey (`os/config/update-trust/`). The engine passes it to
   `gpgv` only after refusing it if it holds any unpinned primary; the package build and the
   image audit refuse the same. It can add a signing subkey, never another root of trust.
3. **Versioning**: one source, `os/VERSION` (first line, a Debian version, baseline `0.1.0~rc1`;
   the transition target is `0.1.0~rc2`). `/etc/writingdeck/release`, package `Version:` fields and
   the media receipt derive from it. Anti-downgrade uses `dpkg --compare-versions`.
4. **WrOS as dpkg packages**: image staging installs WrOS-owned installed-system files through
   dpkg (`dpkg --root`), so `/var/lib/dpkg/status` lists them. Package `wros` (arch amd64)
   owns the installed-system files; live-only files (wros-live-session, wros-install-session,
   `_wros-installer` account) stay plain image files. Update bundles carry new `wros` .debs.
5. **Bundle delivery** (no NIC assumed): a block device whose filesystem label is `WROS-UPDATE`
   (ext4, vfat or iso9660) is mounted read-only, `nodev,nosuid,noexec`, by the narrow privileged verb
   `wros-update-media mount` under `/run/wros-update/media/<n>`; the app lists that path.
   Arbitrary writer-typed paths are accepted only under `/run/wros-update/media/`, `/media/`,
   `/run/media/` and `/mnt/`. **Online (rc2):** `.github/workflows/release-updates.yml` publishes
   the same signed bundle format to `https://j5cfk7qm49-web.github.io/WrOS/updates/` from `v*`
   tags (re-signed weekly, 21 days valid). `wros-update-fetch` downloads it into
   `/var/cache/wros-update/online/<id>`, which is reviewed exactly like media. The same folder
   is a flat apt repository (`/etc/apt/sources.list.d/wros.sources`).
6. **Cancellation boundary**: cancel is possible while `staged` (before the reboot that
   applies it). After `applying` begins there is no cancel; the only action is wait, then
   `recovered`/`failed` handling. APT is never killed because a page closed.
7. **Rescue**: ext4 has no snapshot rollback and the RC does not claim one. Rescue is
   (a) `wros-update repair` (privileged, retained authenticated archives only) and
   (b) `os/scripts/wros-rescue` run from the live ISO: unlocks the encrypted root, mounts it,
   chroots and runs the same repair engine. Documented in `docs/RC_RESCUE.md`.
8. **Live installer**: production path only (Settings -> pkexec `wros-install-session`). The QA
   launcher remains test-only and must not appear in media.

## Update helper interface (owner: WS-A; consumers WS-B, WS-C, WS-F)

Entry points, all root:root 0755 (`/usr/libexec/`): `wros-update` (engine + unprivileged verbs),
`wros-update-stage`, `wros-update-cancel`, `wros-update-repair`, `wros-update-media`,
`wros-update-fetch` (thin pkexec launchers: exact argc, strict tokens, `PKEXEC_UID` present, sanitized env, import
the engine from `/usr/lib/wros-update/engine.py`). `run-offline-boot` and `reconcile` refuse
`PKEXEC_UID`.

JSON on stdout (UTF-8, one object, no secrets):

* `wros-update review <abs-dir>` (unprivileged) ->
  `{"status":"reviewed","review_id":"<64 hex>","signer":"<fpr>","suite":"..","date":"..","valid_until":"..",
    "packages":[{"name","from_version"|null,"to_version","action":"install|upgrade|reinstall"}],
    "removals":[],"backup_recommended":true}`; `review_id = sha256(InRelease||0x00||Packages)`.
* `wros-update-stage <abs-dir> <review_id>` -> `{"status":"staged","transaction_id","review_id","packages":N,"restart_required":true}`;
  refuses if the freshly verified `review_id` differs.
* `wros-update-cancel` -> `{"status":"cancelled"|"none","message"}`.
* `wros-update-repair` -> `{"status":"recovered"|"clean"|"failed","message"}`.
* `wros-update-media mount` -> `{"status":"mounted","path":"/run/wros-update/media/0"}` | `{"status":"none"}`.
* `wros-update check-online` (unprivileged; InRelease and Packages only) ->
  `{"status":"available|current|clock|unreachable|invalid","version","installed","clock"}`.
* `wros-update-fetch` -> `{"status":"fetched","path":"/var/cache/wros-update/online/<16 hex>","version"}`;
  refuses a version not newer than dpkg's installed `wros`.
* `wros-update recovery-status` (unprivileged, reads only `/var/lib/wros-update/public/status.json`
  0644 and world-readable dpkg state) -> existing `RecoveryInfo` fields plus
  `"update_state":"idle|staged|applying|failed|recovered|completed|interrupted"`.

Journal states: `staged` -> `applying` -> `completed` | `failed` | `interrupted` -> `recovered`.
Foreign `/system-update` markers are never touched.

Rust (owner WS-C): `Request::ReviewUpdateBundle{bundle_dir}`, `Request::StageUpdates{bundle_dir,review_id}`,
`Request::FindUpdateMedia`, `Reply::UpdateReview(UpdateReview)`; `CancelStagedUpdates`/`RecoverUpdates`
invoke the launchers through `pkexec --disable-internal-agent`.

## Polkit

Actions `org.wros.update.{stage,cancel,repair,media,fetch}`, one exec path each, implicit result
`yes` only for `subject.local && subject.active && isInGroup("writingdeck")` (rules file
`os/config/50-wros-update.rules`). The in-app confirm screen (default Cancel) is the human gate.

## Ownership

| WS | Owns |
|----|------|
| A update engine | `os/scripts/wros-update*`, `os/tests/test_update*.py`, `tools/build-update-bundle.py`, `os/config/org.wros.update.policy`, `os/config/50-wros-update.rules`, `os/systemd/wros-offline-update.service`, `os/systemd/wros-update-reconcile.service`, `os/scripts/wros-rescue`, `docs/RC_RESCUE.md` |
| B packaging/image | `packaging/`, `os/VERSION`, `os/scripts/install-files.sh`, `tools/audit-native-image.py`, `os/tests/test_native_image.py`, `tools/build-wros-debs.py` |
| C Rust app | `crates/writingdeck-sys/**`, `src/app/**`, `src/shell_ui_tests.rs` |
| D install supervisor | `os/scripts/wros-install-session`, `os/tests/test_install_session.py` |
| E ISO driver/media audit | `os/scripts/build-*.sh`, `os/config/packages.txt`, `tools/audit-installer-media.py`, `tools/audit-live-session.py`, `tools/stage-native-installer.py`, `tools/verify-debian-installer.py`, `os/tests/test_installer_media.py` |
| F VM drivers | `os/tests/*vm*.py`, `os/tests/installed_boot.py`, new drivers |

Docs and `docs/RC_GAPS.md` are reconciled last by the integrator.
