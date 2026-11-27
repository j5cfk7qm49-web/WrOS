# WrOS release checklist

## Build and qualify the candidate

1. Record the candidate commit and set `os/VERSION`.
2. On the Linux Nix builder, run the software and VM checks in
   [NIXOS_QUALIFICATION](NIXOS_QUALIFICATION.md). Keep the qualification artifacts
   and name each failure that remains.
3. Build the ISO and unsigned closure bundle:

   ```sh
   nix build --out-link result-iso .#wros-iso
   nix build --out-link result-update-bundle .#wros-update-bundle
   ```

   Release builds offering phone linking must set `WROS_GOOGLE_CLIENTS` to the
   approved Google client configuration file and add `--impure` to both builds.
4. Sign `result-update-bundle` with `tools/build-nixos-update-bundle.py` using the
   external release key, as in [qualification step 6](NIXOS_QUALIFICATION.md#step-6-build-the-offline-live-iso-and-binary-cache-update-bundle).
   Archive the signed directory's contents as `wros-nixos-updates.tar`. Verify
   that tarball with an empty verification directory:

   ```sh
   python3 -I tools/publish-nixos-updates.py check-bundle "$tarball" "$verified_bundle" "$version"
   ```

   Keep the tarball SHA-256 and release notes with the qualification artifacts.
5. Review the candidate's hardware results before approving the tag. The release
   workflow serves the verified `wros-nixos-updates-thin.tar` when present
   and falls back to `wros-nixos-updates.tar`.

rc13 was the last Debian release. Git history preserves its release procedure.

## Check installation without Internet

The NixOS ISO contains the complete installed-system closure and supported
firmware. Run `nix build -L .#checks.x86_64-linux.install-encrypted` and perform
the actual ISO checks in [NIXOS_QUALIFICATION](NIXOS_QUALIFICATION.md). Verify
installation from the medium without relying on network package downloads.
Network time and update metadata checks are separate services. Keep any IPv4
capture as an additional network audit, labeled with the exact ISO and harness
used. Verify Wi-Fi provisioning separately on the laptop.

## HP Stream hardware checks

Run [HARDWARE_CHECKLIST](HARDWARE_CHECKLIST.md) on the laptop: typing speed,
lid, lock and sleep, Wi-Fi after sleep and over a day, the school network, low
battery, shutdown during a backup, the recovery boot, and what an update must
carry over. Record the results in its table.

## Verify a live update after publication

1. Check that Pages serves `nixos-updates/InRelease`,
   `nixos-updates/closure-manifest.json` and every required cache file from the
   verified bundle selected by the workflow. Verify the signed manifest and compare
   downloaded cache-file sizes and SHA-256 hashes with the bundle.
2. Use a disposable older WrOS installation whose app accepts the NixOS updater
   replies. rc14 and rc15 cannot install updates from Settings; follow
   [Update an installed system](INSTALLATION.md#update-an-installed-system) for
   their routes to a supported baseline. Open **Settings › Updates and recovery › Updates**
   and choose **Check for WrOS updates**. Review the version, signer and validity
   dates. Confirm staging, restart, unlock and check that Settings reports the
   completed update and that the original project is intact. Test the Settings path explicitly;
   a shell-driven `update-goback` VM result does not establish this UI path.
3. On the HP Stream, confirm the recovery menu is reachable with Esc at boot.
   Try **Show what happened** and **Continue to the normal login**.

## Renew the NixOS channel signature

The NixOS channel's `InRelease` is signed offline with a `Valid-Until` set by
`tools/build-nixos-update-bundle.py --valid-days`. Publish a new release or an
offline re-sign before that deadline, at most 30 days after the last signing.
The weekly run checks the live channel and fails when fewer than 10 days remain,
opening an issue for that expiry date if needed.


## NixOS release candidate qualification (rc14 and later)

Before tagging a NixOS release candidate, evaluate and build the flake outputs
and execute the ordered steps in [NIXOS_QUALIFICATION](NIXOS_QUALIFICATION.md).
