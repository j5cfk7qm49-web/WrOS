# WrOS NixOS updates and atomic generation rollback (rc14 and later)

This document describes how [`nixos/scripts/wros-update-engine.py`](../nixos/scripts/wros-update-engine.py) and [`nixos/modules/updates.nix`](../nixos/modules/updates.nix) implement offline and online updates on NixOS while preserving the OpenPGP trust model, append-only journal, probation rules, and CLI surface of [`os/scripts/wros-update-engine.py`](../os/scripts/wros-update-engine.py).

## 1. Shared trust model & store-backed trust anchors (`U1`, `U6`, `U8`)

`nixos/scripts/wros-update-engine.py` imports the signature verification, journal (`Store`), probation, and media-mounting implementation from `debian_engine.py` (installed from `os/scripts/wros-update-engine.py` with Nix store tool and keyring paths substituted by `nixos/pkgs/helpers.nix`, `U1`):

- **Store-backed trust anchors (`U6`)**: [`nixos/modules/updates.nix`](../nixos/modules/updates.nix) declares `options.wros.updates.trustDir` (defaulting to `wrosRoot + "/os/config/update-trust"`, and overridden only in `nixos/tests/update-goback.nix` to point at `nixos/tests/keys`). The declared files are `wros-archive-keyring.gpg` (`DEFAULT_KEYRING`) and `update-signers` (`DEFAULT_SIGNERS`, pinned 40-hex primary fingerprint `B9F5B97C9370239CC07B425F2CB426FC6D27CA65`). `environment.etc` installs activation-managed regular files under `/etc/wros/update-trust` with mode `0444`. `resolve_trust_anchor()` also supports symlinks into `/nix/store`, resolving them when present. Generation activation refreshes the declared trust files.
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
  closure-manifest.json         # Schema 2 (wros-nixos-cache-v2), with cache_files[] and firmware{}
  cache/
    nix-cache-info              # Local Nix binary cache metadata
    <store-hash>.narinfo        # One .narinfo per store path in the bundle
    nar/
      <file-hash>.nar.zst       # Zstandard-compressed NAR archive referenced by the .narinfo
```

- **Full vs. delta bundles (`U5`)**:
  - A **full bundle** (`baseline: null`, built when `baselineToplevel = null` in `nixos/pkgs/update-bundle.nix` or without `--baseline-toplevel` in `tools/collect-nixos-update-runtime.py`) carries the complete closure of `toplevel` for offline USB recovery or multi-release jumps.
  - A **delta bundle** (`baseline: "/nix/store/...-nixos-system-wros-..."`, built when `baselineToplevel` / `--baseline-toplevel` is supplied) prunes every `.narinfo` and `.nar.zst` already present in `baseline`'s closure so the bundle carries only store paths changed since the baseline release.
  - Fetching and staging can omit system cache files for valid existing non-toplevel store paths.
- **Firmware (rc33 and later)**: The signed `closure-manifest.json` has a
  `firmware` object keyed by package name. Each entry gives the `store_path`
  and the package's `files`, with the cache paths, SHA-256 hashes and sizes of
  its `.narinfo` and compressed NAR. The HP Stream seed entries have empty
  `files` lists because their files are indexed in `cache_files` for older
  updaters. Both the delta and thin bundles retain the firmware cache files
  for hosting on Pages.

## 3. Pre-import verification & import through Nix (`U2`)

Because `/nix/store` is mounted read-only on NixOS 26.05, writing directly into `/nix/store` with `nix-store --restore` fails (`Read-only file system`). Instead, `nixos/scripts/wros-update-engine.py` verifies every file before handing the local binary cache to `nix copy`:

1. **OpenPGP signature & signer pin**: `verify_signed_metadata` verifies `InRelease` (or `Release` + `Release.gpg`) with `gpgv` against the store-backed `wros-archive-keyring.gpg` and confirms the primary key fingerprint matches `update-signers`.
2. **Release validity & replay guard**: `parse_release` checks `Origin: WrOS`, `Suite: wros-nixos-updates`, `Date`, and `Valid-Until` (`<= 30 days`), and refuses any bundle whose `Date` is not newer than `replay.json`.
3. **Manifest digest check**: `closure-manifest.json` is verified against the exact SHA-256 and byte size recorded in the signed `Release` file.
4. **Every supplied binary-cache file verified by SHA-256**: `verify_closure_archives` checks the SHA-256 and byte size of every supplied indexed file, including files for existing store paths and files in the `firmware` section, and rejects any unindexed file under `cache/`. For the system cache, `allow_skipped_present` permits omitting files for valid existing non-toplevel store paths. The toplevel files and `cache/nix-cache-info` remain required. Firmware files can be fetched selectively and are verified against the signed manifest before import.
5. **Import via `nix copy` (`U2`)**: Only after steps 1–4 succeed does `import_verified_closure` run:
   ```sh
   nix --extra-experimental-features nix-command copy --from file:///var/lib/wros-update/staged/cache --no-check-sigs --no-recursive <toplevel> <other-staged-store-paths...>
   nix-store --verify-path <toplevel>
   ```
   The arguments include the toplevel and every indexed `store_path` whose `.narinfo` is present in the staged cache. `--no-recursive` avoids walking baseline paths omitted from the source cache; those paths are already valid locally. `--no-check-sigs` is safe here strictly because `verify_closure_archives` has just verified the SHA-256 of `nix-cache-info`, every `.narinfo`, and every `.nar.zst` in `/var/lib/wros-update/staged/cache` against the OpenPGP-signed `closure-manifest.json` immediately before invoking `nix copy`.

## 4. Design decision: two-restart `/system-update` apply flow (`U8`) & disk management (`U4`)

### Stage now, switch in `system-update.target` (`U8`)

WrOS retains the two-phase offline apply flow (`/system-update` -> `system-update.target` -> `wros-offline-update.service`). For bundles with a `firmware` section, staging imports the verified system and prepares its firmware while the update source is available (see section 6).

- **Why**: The offline apply service keeps the generation switch outside the
  writing session. Bundles without firmware also defer the store import to
  that service to reduce competition for eMMC I/O and RAM on the HP Stream 11.
- **Restart and disk cost**: Applying a full system update takes two restarts. The first enters `system-update.target`, where `wros-offline-update.service` imports any remaining verified cache paths, sets `/nix/var/nix/profiles/system`, runs `<toplevel>/bin/switch-to-configuration boot`, and restarts into the new generation. The journal records `staged` -> `applying` -> `completed`. Until apply finishes, `/var/lib/wros-update/staged/cache` can hold compressed `.nar.zst` files that were copied there during staging. App-only updates (`closure_live_apply_check` where kernel, initrd, and systemd units are unchanged) can still be applied live via `wros-update-apply` without a system restart.

### Disk space checks, cache cleanup & garbage collection (`U4`)

To protect the 32 GB eMMC from filling up across releases:

1. **Free-space checks before fetching and staging**: `require_free_space()` checks the download-cache filesystem before fetching and the staging filesystem before copying. It requires the compressed files' size plus 512 MiB free. It does not separately budget the unpacked store import.
2. **Removal of staged NARs after import and generation switch**: After the closure import and generation switch both succeed, `apply_staged_transaction` deletes `/var/lib/wros-update/staged/cache` (`shutil.rmtree(store.staged / 'cache')`) before rotating `staged/` to `installed/`. As a result, `installed/` and `previous/` retain only the small metadata files (`Release`, `InRelease`, `closure-manifest.json`, `manifest.json`) and never duplicate compressed `.nar.zst` archives already in `/nix/store`.
3. **Generation pruning and garbage collection once healthy**: `update_gc_roots` maintains three symlink GC roots in `/nix/var/nix/gcroots/wros/` (`running`, `previous`, `staged`). When the new generation passes probation (`wros-update-healthy ready` -> `report_health('ready')`), `prune_old_generations_and_gc` deletes old profile generations, prunes their firmware sets and roots, then runs `nix-collect-garbage` to free unreferenced store paths.

## 5. Online channel hosting, GitHub Pages limits & streaming downloads (`D2`, `U8`)

### Where online update files are hosted (`D2`)

The current updater fetches online channel files from GitHub Pages. It refuses redirects,
so GitHub Release download URLs cannot supply its NAR files. Publish deltas that fit the
Pages checks; use release assets to distribute offline bundles. The manifest supports
per-file `url` entries, but not `archive_base_url`, and Release download URLs remain
unusable by this opener.

### Current fetch restriction

The engine uses `_NoRedirect`. Every online channel file must be available directly from
Pages. The [publishing workflow](../.github/workflows/release-updates.yml) checks the complete
offline-signed channel, rejects files matching `find -size +99M`, and deploys
`nixos-updates/` and the static pages together. It selects the thin release asset
when available and falls back to the full asset. A deploy requires both the
verified channel and the static pages.

Historical measurement from publishing rc15: the rc14-to-rc15 delta contained 44 store
paths, about 120 MiB before compression, the largest 40 MiB. These are historical sizes,
not a size claim for the current release. Delta publication uses `--baseline-toplevel` and
`--baseline-store-paths` in `tools/build-nixos-update-bundle.py`. Full bundles remain an
offline USB option.

There is no CI signing key for this channel, so its `Valid-Until` (at most 30 days) lapses
unless a release is signed again offline before then.

### Streaming downloads with bounded memory (`U8`)

Because the target laptop has 4 GB of RAM, `http_stream_to_file(url, dest_path, expected_size, expected_sha256, opener=None, timeout=120)` in [`nixos/scripts/wros-update-engine.py`](../nixos/scripts/wros-update-engine.py) streams every `.narinfo` and `.nar.zst` response body directly to disk in **64 KiB chunks** while incrementally updating `hashlib.sha256()`, enforcing the exact byte size and SHA-256 from `closure-manifest.json` before moving the temporary file into `/var/cache/wros-update/online/`. No `.nar.zst` archive is ever buffered in memory.

## 6. Per-machine firmware sets (rc33 and later)

The signed system toplevel is the same on every laptop. Its `wros-firmware.json`
maps supported kernel modules to driver packages and records each package's
store path, NAR hash and size, and whether it is needed in the initrd.
The WrOS live ISO carries all supported packages. During an offline install,
the installer detects the target laptop's hardware, copies the selected packages
into the target store, and writes the first generation's set.

An online update first selects packages from the running system's detection and
existing set, and reuses paths already in the store. Staging checks again against
the target kernel, imports any additional packages from the verified channel or
the offline bundle, and finishes the new set before the bootloader is updated.
The set carries forward the running generation's packages that are available for
the target. If a needed package is missing locally and the release manifest
lacks it, staging reports its name under `firmware_missing`.

Each generation has its own symlink tree at
`/nix/var/wros/firmware/<toplevel store name>/lib/firmware/`. Its packages are
kept alive by GC roots under `/nix/var/nix/gcroots/wros-firmware/`. Activation
links `/lib/firmware` to the generation's tree. GRUB loads a second initrd for
that generation with only the packages needed in early boot, such as graphics,
under `/wros-firmware/`. Writing the archive requires leaving at least 64 MiB
free on `/boot`. **Go back** boots the previous generation with its own set.

For a direct online update from rc32 or earlier, the signed system closure carries
the HP Stream's seed (`rtw88`, `i915-glk`, `sof`) alongside the common wireless
regulatory database. The old updater imports the seed with the system. The
bootloader hook and activation build a set from the available packages if the
generation has no set yet. The seed is also on the installed system's fallback
firmware path.

To see what WrOS detects on a running laptop, open the maintenance console and run:

```sh
wros-firmware detect --system /run/current-system
```

The command prints `packages` (driver packages selected from device aliases and
loaded modules) and `unknown` (aliases the kernel modules could not resolve).
Use the [hardware checklist](HARDWARE_CHECKLIST.md#8-after-an-update) to verify
Wi-Fi, display and sound after an update and after going back. For hardware
plugged in later, open **Settings › Updates and recovery › Updates › Firmware for
a new device**. WrOS uses the signed channel or, if it cannot reach the site,
checks a WrOS USB stick. The stick's package must match the store path, NAR
SHA-256 and size in the running system's firmware table. If the channel names
a different store path for a needed package, update WrOS first.
