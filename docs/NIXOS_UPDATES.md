# WrOS NixOS updates and atomic generation rollback (`0.1.0~rc14`)

This document describes how [`nixos/scripts/wros-update-engine.py`](../nixos/scripts/wros-update-engine.py) and [`nixos/modules/updates.nix`](../nixos/modules/updates.nix) implement offline and online updates on NixOS while preserving the OpenPGP trust model, append-only journal, probation rules, and CLI surface of [`os/scripts/wros-update-engine.py`](../os/scripts/wros-update-engine.py).

## 1. Shared trust model & store-backed trust anchors (`U1`, `U6`, `U8`)

`nixos/scripts/wros-update-engine.py` imports the signature verification, journal (`Store`), probation, and media-mounting implementation from `debian_engine.py` (installed from `os/scripts/wros-update-engine.py` with Nix store tool and keyring paths substituted by `nixos/pkgs/helpers.nix`, `U1`):

- **Store-backed trust anchors (`U6`)**: [`nixos/modules/updates.nix`](../nixos/modules/updates.nix) declares `options.wros.updates.trustDir` (defaulting to `wrosRoot + "/os/config/update-trust"`, and overridden only in `nixos/tests/update-goback.nix` to point at `nixos/tests/keys`). The module installs `/etc/wros/update-trust/wros-archive-keyring.gpg` (`DEFAULT_KEYRING`) and `/etc/wros/update-trust/update-signers` (`DEFAULT_SIGNERS`, pinned 40-hex primary fingerprint `67978962A538C3B92B459792EC6B30574351C793`) via `environment.etc` with `mode = "0444"` (regular files in `/nix/store`), and `resolve_trust_anchor()` in `nixos/scripts/wros-update-engine.py` resolves each symlink directly to its immutable `/nix/store` regular file before calling `require_trusted_file()`. Rotating the keyring in a future release updates `/etc/wros/update-trust/*` automatically when that generation activates.
- **Suite & validity constants (`U8`)**:
  - `PINNED_SUITES = ('wros-nixos-updates',)` (never accepts the Debian `wros-updates` suite).
  - `MAX_VALIDITY = timedelta(days=30)`
  - `MAX_STAGED_AGE = timedelta(days=30)`
  - `CLOCK_SKEW = timedelta(minutes=5)`
  - `BOOT_CLOCK_SKEW = timedelta(hours=1)`
  - `PROBATION_FAILED_LOGINS = 2`
- **Installed version & downgrade refusal (`U3`)**: [`nixos/modules/base.nix`](../nixos/modules/base.nix) writes `environment.etc."wros/version".text = "${version}\n";` (`DEFAULT_VERSION_FILE = Path('/etc/wros/version')`). `compare_versions(left, relation, right)` compares Debian/WrOS tilde versions in pure Python (`0.1.0~rc12 < 0.1.0~rc14 < 0.1.0~rc15 < 0.1.0`), and `verify_open_nixos_bundle` refuses any bundle whose version is older (`downgrade`) or equal (`reinstall`) to `/etc/wros/version`.

## 2. Binary-cache bundle layout (`U2`, `U5`)

Built by [`nixos/pkgs/update-bundle.nix`](../nixos/pkgs/update-bundle.nix) or [`tools/collect-nixos-update-runtime.py`](../tools/collect-nixos-update-runtime.py) (using `nix --extra-experimental-features nix-command copy --to file://<bundle>/cache?compression=zstd`) and signed outside the repository by [`tools/build-nixos-update-bundle.py`](../tools/build-nixos-update-bundle.py):

```text
<bundle>/
  InRelease                     # OpenPGP clearsigned Release metadata
  Release                       # Origin: WrOS, Suite: wros-nixos-updates, Date, Valid-Until, SHA256 of closure-manifest.json
  Release.gpg                   # Detached OpenPGP signature
  closure-manifest.json         # Schema 2 (format: wros-nixos-closure-v2): version, system, toplevel, baseline, cache_files[], paths[]
  cache/
    nix-cache-info              # Local Nix binary cache metadata
    <store-hash>.narinfo        # One .narinfo per store path in the bundle
    nar/
      <file-hash>.nar.zst       # Zstandard-compressed NAR archive referenced by the .narinfo
```

- **Full vs. delta bundles (`U5`)**:
  - A **full bundle** (`baseline: null`, built when `baselineToplevel = null` in `nixos/pkgs/update-bundle.nix` or without `--baseline-toplevel` in `tools/collect-nixos-update-bundle.py`) carries the complete closure of `toplevel` for offline USB recovery or multi-release jumps.
  - A **delta bundle** (`baseline: "/nix/store/...-nixos-system-wros-..."`, built when `baselineToplevel` / `--baseline-toplevel` is supplied) prunes every `.narinfo` and `.nar.zst` already present in `baseline`'s closure so the bundle carries only store paths changed since the baseline release.
  - During both local verification and online fetching (`fetch_online`), `verify_closure_archives` and `fetch_online` check `store_has_path(store_path)` and skip downloading or requiring `.narinfo`/`.nar.zst` files for store paths already present in `/nix/store`.

## 3. Pre-import verification & import through Nix (`U2`)

Because `/nix/store` is mounted read-only on NixOS 26.05, writing directly into `/nix/store` with `nix-store --restore` fails (`Read-only file system`). Instead, `nixos/scripts/wros-update-engine.py` verifies every file before handing the local binary cache to `nix copy`:

1. **OpenPGP signature & signer pin**: `verify_signed_metadata` verifies `InRelease` (or `Release` + `Release.gpg`) with `gpgv` against the store-backed `wros-archive-keyring.gpg` and confirms the primary key fingerprint matches `update-signers`.
2. **Release validity & replay guard**: `parse_release` checks `Origin: WrOS`, `Suite: wros-nixos-updates`, `Date`, and `Valid-Until` (`<= 30 days`), and refuses any bundle whose `Date` is not newer than `replay.json`.
3. **Manifest digest check**: `closure-manifest.json` is verified against the exact SHA-256 and byte size recorded in the signed `Release` file.
4. **Every binary-cache file verified by SHA-256**: `verify_closure_archives` checks the SHA-256 and byte size of `cache/nix-cache-info`, every `cache/<hash>.narinfo`, and every `cache/nar/<filehash>.nar.zst` listed in `closure-manifest.json` (for all store paths not already in `/nix/store`), and rejects any unindexed file under `cache/`.
5. **Import via `nix copy` (`U2`)**: Only after steps 1–4 succeed does `import_verified_closure` run:
   ```sh
   nix --extra-experimental-features nix-command copy --from file:///var/lib/wros-update/staged/cache --no-check-sigs <toplevel>
   nix-store --verify-path <toplevel>
   ```
   `--no-check-sigs` is safe here strictly because `verify_closure_archives` has just verified the SHA-256 of `nix-cache-info`, every `.narinfo`, and every `.nar.zst` in `/var/lib/wros-update/staged/cache` against the OpenPGP-signed `closure-manifest.json` immediately before invoking `nix copy`.

## 4. Design decision: two-restart `/system-update` apply flow (`U8`) & disk management (`U4`)

### Recorded design decision: stage now, import & switch in `system-update.target` (`U8`)

WrOS retains the two-phase offline update flow (`/system-update` -> `system-update.target` -> `wros-offline-update.service`) rather than importing into `/nix/store` while the writer is actively editing:

- **Why**: Keeping `nix copy` and `switch-to-configuration boot` inside `wros-offline-update.service` during `system-update.target` avoids competing with the writer's interactive session or background Restic backups for eMMC I/O and RAM on the 4 GB / 32 GB HP Stream 11, and matches the Debian `wros-offline-update.service` state machine and journal lifecycle (`staged` -> `applying` -> `completed`).
- **Restart and disk cost**: Applying a full system update takes two restarts (first into `system-update.target` where `wros-offline-update.service` imports the verified cache, sets `/nix/var/nix/profiles/system`, runs `<toplevel>/bin/switch-to-configuration boot`, and reboots into the new generation). While staged before the restart, `/var/lib/wros-update/staged/cache` temporarily holds the compressed `.nar.zst` files of the new store paths not already in `/nix/store`. App-only updates (`closure_live_apply_check` where kernel, initrd, and systemd units are unchanged) can still be applied live via `wros-update-apply` without a system restart.

### Disk space checks, cache cleanup & garbage collection (`U4`)

To protect the 32 GB eMMC from filling up across releases:

1. **Pre-stage free-space check**: `check_free_disk_space()` calls `os.statvfs` on `/var/lib/wros-update` and `/nix/store` before staging or fetching an update and refuses with a clear error if free space would fall below `MIN_FREE_BYTES` (`512 MiB`) plus the bytes required by the bundle.
2. **Immediate removal of staged NARs after store import**: Immediately after `import_verified_closure` imports the closure into `/nix/store`, `apply_staged_transaction` deletes `/var/lib/wros-update/staged/cache` (`shutil.rmtree(store.staged / 'cache')`) before rotating `staged/` to `installed/`. As a result, `installed/` and `previous/` retain only the small metadata files (`Release`, `InRelease`, `closure-manifest.json`, `manifest.json`) and never duplicate compressed `.nar.zst` archives already in `/nix/store`.
3. **Generation pruning and garbage collection once healthy**: `update_gc_roots` maintains three symlink GC roots in `/nix/var/nix/gcroots/wros/` (`running`, `previous`, `staged`). When the new generation passes probation (`wros-update-healthy ready` -> `report_health('ready')`), `prune_old_generations_and_gc` runs `nix-env -p /nix/var/nix/profiles/system --delete-generations +2` and `nix-collect-garbage` to free all unreferenced store paths.

## 5. Online channel hosting, GitHub Pages limits & streaming downloads (`D2`, `U8`)

### Where online update files are hosted (`D2`)

GitHub Pages enforces a **100 MB per-file limit** and a **~1 GB total site limit**, and already hosts the Debian `updates/` channel for `v0.1.0-rc13` devices. The NixOS online channel therefore splits small metadata/delta files and large `.nar.zst` archives across two GitHub endpoints from the same repository (`https://github.com/j5cfk7qm49-web/WrOS`):

| File(s) | Typical size | Hosted at | Purpose |
|---|---|---|---|
| `InRelease`, `Release`, `Release.gpg` | `< 16 KiB` | GitHub Pages: `https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/` | OpenPGP-signed release metadata (`Origin: WrOS`, `Suite: wros-nixos-updates`, SHA-256 of `closure-manifest.json`) fetched by `wros-update check-online` and `wros-update-fetch` |
| `closure-manifest.json` | `10–250 KiB` | GitHub Pages: `https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/closure-manifest.json` | Lists `toplevel`, `baseline`, `archive_base_url` (optional GitHub Release asset base URL), and the SHA-256 and byte size of every `.narinfo` and `.nar.zst` |
| `cache/nix-cache-info` and `cache/<hash>.narinfo` | `< 4 KiB` each | GitHub Pages: `https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/cache/` | Nix binary-cache metadata for each store path |
| Delta `cache/nar/<filehash>.nar.zst` (`< 100 MB`) | `1–80 MiB` | GitHub Pages (`https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/cache/nar/`) or GitHub Releases (`archive_base_url`) | Compressed NAR archives for store paths not already in `/nix/store` |
| Large `.nar.zst` archives (`>= 100 MB`, e.g., `llvm-lib`, `wros-firmware`, `mesa`, `wros-prose-runtime`) and full recovery bundles (`wros-nixos-updates.tar`) | `100 MiB – 1.2 GiB` | GitHub Releases: `https://github.com/j5cfk7qm49-web/WrOS/releases/download/v<version>/<file>` (which redirects over HTTPS to `objects.githubusercontent.com` / `release-assets.githubusercontent.com`, supporting up to 2 GiB per asset) | Hosted as release assets referenced by `archive_base_url` / `entry["url"]` in `closure-manifest.json`, keeping the GitHub Pages site well below 100 MB per file and 1 GB total |

### Streaming downloads with bounded memory (`U8`)

Because the target laptop has 4 GB of RAM, `http_stream_to_file(url, dest_path, limit=..., expected_sha256=..., expected_size=...)` in [`nixos/scripts/wros-update-engine.py`](../nixos/scripts/wros-update-engine.py) streams every `.narinfo` and `.nar.zst` response body directly to disk in **64 KiB chunks** while incrementally updating `hashlib.sha256()`, enforcing the exact byte size and SHA-256 from `closure-manifest.json` before moving the temporary file into `/var/cache/wros-update/online/`. No `.nar.zst` archive is ever buffered in memory.
