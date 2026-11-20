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
   workflow expects the verified `wros-nixos-updates.tar` asset.

## Debian builds, rc13 and earlier

The following package, RC-chain and PTY instructions belong to Debian builds.

1. Record the candidate commit and set `os/VERSION` (the packaging test checks
   it). Run `tools/rc-check.sh` in the Debian 13 chroot and keep its
   `summary.txt`; name each failure that remains (the prose-runtime tests fail
   without the provisioned runtime).
2. In the Debian build environment, run `cargo build --release --locked
   --no-default-features --features gui-drm,gui-wgpu,audio -p grimoire-tui -p wros-greeter
   -p wros-boot`. Copy those three binaries into a clean directory and give it
   to `tools/build-wros-debs.py --binaries-dir "$binary_dir" --output "$package_dir"`.
   Include the approved Google client configuration for release builds that
   offer phone linking.
3. Choose the bundle's Debian packages with `tools/collect-update-runtime.py
   "$wros_deb" "$baseline_dpkg_status" "$work"`, where the status file is
   `/var/lib/dpkg/status` from the saved installation being upgraded. It plans
   the offline install against that installation, downloads each missing or
   older package from Debian 13 with its updates and security archives, and
   refuses any package older than Debian's newest (#117). Sign the wros
   package and `$work/runtime/*.deb` with `tools/build-update-bundle.py`
   using the release key outside the source tree. Archive the signed index
   files and `pool/` as `wros-updates.tar`. Run `python3 -I
   tools/publish-updates.py check-bundle "$tarball" "$verified_bundle" "$version"`
   with an empty verification directory. Keep the tarball SHA-256 and release notes.
4. Run the tracked VM chain against the verified bundle and the saved
   baseline disk. It updates through Settings and tests the installed
   features, graphical startup and the paths back to Debian's prompt. Keep
   each `result.json` and the screen captures.

   ```sh
   python3 os/tests/rc_vm_chain.py --install-run "$install_run" \
     --bundle "$verified_bundle" --expect "$version" \
     --start-disk "$baseline_disk" --start-version "$baseline_version"
   ```

   Generate the matching startup and writing glyph atlases for the graphical
   checks. [Graphical session test setup](GRAPHICAL_SESSION.md)
5. Review the VM results and hardware checklist before approving the tag.
   Release tags use the checked `wros-updates.tar` as their live payload.

The local layer-two checks use the actual editor in a private pseudo-terminal.
In the Debian chroot, build with `cargo build --offline --locked
--no-default-features --features pty-qa -p grimoire-tui`. Set `GRIMOIRE_BINARY`
to that debug executable and run `WROS_QA_PTY=1 PYTHONPATH=os/tests python3 -m
unittest test_rc7_pty`. The fixture locker tests input retirement and typeahead
isolation. The QEMU feature phase tests the installed locker and PAM.

The retained Debian installation network audit uses QEMU, `iproute2` and
`tcpdump`, with a wired virtual NIC in a fresh network namespace without an
Internet connection:

```sh
sudo unshare --net runuser -u builder -- python3 os/tests/rc_vm_phases.py \
  install --iso "$iso" --layout colemak --audit-network
```

Keep its result and private `install.pcap` with the exact Debian ISO and harness.
The VM uses a disposable disk; this audit covers IPv4 during wired installation.

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
   verified `wros-nixos-updates.tar`. Verify the signed manifest and compare
   downloaded cache-file sizes and SHA-256 hashes with the bundle.
2. Use a disposable older WrOS installation whose app accepts the NixOS updater
   replies. rc14 and rc15 cannot install updates from Settings; follow
   [Update an installed system](INSTALLATION.md#update-an-installed-system) for
   their routes to a supported baseline. Open **Settings › Updates and recovery › Updates**
   and choose **Check for WrOS updates**. Review the version, signer and validity
   dates. Confirm staging, restart, unlock and check that Settings reports the
   completed update and that the original book is intact. Test the Settings path explicitly;
   a shell-driven `update-goback` VM result does not establish this UI path.
3. On the HP Stream, confirm the recovery menu is reachable with Esc at boot.
   Try **Show what happened** and **Continue to the normal login**.

## Verify the retained Debian channel

Check that Pages serves `updates/InRelease`, `updates/Release`,
`updates/Release.gpg`, `updates/Packages` and the Debian bundle's packages. Run
`python3 -I tools/publish-updates.py published <updates-url> <empty-dir>` in a
fresh local directory. Compare each downloaded package's SHA-256 with its entry
in the offline Debian bundle.

## Set up the weekly signing subkey

This signing path refreshes Debian-channel metadata during an eligible
deployment; it does not sign the NixOS bundle.

These steps are for the owner. The offline release key stays in the offline
keyring. Start with a release that will distribute the new public subkey.

1. In the offline keyring, add a signing-only subkey to the existing pinned
   primary. Choose the expiry and record it for renewal. In these commands,
   `PRIMARY` is the existing primary fingerprint and `SUBKEY` is the new
   signing subkey fingerprint from the listing.

   ```sh
   gpg --quick-add-key "$PRIMARY" ed25519 sign 1y
   gpg --list-keys --with-subkey-fingerprint "$PRIMARY"
   gpg --export "$PRIMARY" > os/config/update-trust/wros-archive-keyring.gpg
   ```

2. Check that the public export contains the new signing subkey and the same
   pinned primary. Commit the public keyring with the workflow on the default
   branch. Build and publish an offline-signed release that carries this public
   keyring. Install that release on the laptop before enabling weekly signing.
   When testing rollback across this rotation, use a target version that can
   read the retained rollback keyring. Older targets need a fresh bundle
   signed directly by the pinned primary with a date above their replay floor.
3. On the offline machine, export the exact secret subkey to a private file.
   If it has a passphrase, import this subkey-only export into a disposable
   GnuPG home and use `gpg --edit-key "$PRIMARY"` with `passwd` there to clear
   the passphrase from the CI copy. Export again from that home. The installed
   GnuPG agent can prompt locally for these owner steps. [GnuPG subkey instructions](https://www.gnupg.org/faq/gnupg-faq.html)

   ```sh
   umask 077
   gpg --armor --output "$CI_EXPORT" --export-secret-subkeys "${SUBKEY}!"
   ```

4. Test the export in a fresh disposable GnuPG home. It should list a primary
   stub and one usable signing subkey. Make a batch signature of a disposable
   file using `--local-user "${SUBKEY}!"` and verify it with the committed
   public keyring. Add the export as the Actions secret
   `WROS_UPDATE_SIGNING_SUBKEY` for the `github-pages` environment. Remove the
   temporary export and disposable keyring after setup.
5. This signing path refreshes Debian-channel metadata during an eligible
   deployment. The current weekly schedule names no NixOS release and deploys
   nothing; it does not renew either published channel automatically. A NixOS
   channel refresh requires a newly signed bundle and a tag or dispatch naming
   its release.

The NixOS channel's `InRelease` is signed offline with a `Valid-Until` set by
`tools/build-nixos-update-bundle.py --valid-days`. Publish a new release or an
offline re-sign before that deadline, at most 30 days after the last signing.
The weekly run checks the live channel and fails when fewer than 10 days remain,
opening an issue for that expiry date if needed.


## NixOS release candidate qualification (rc14 and later)

Before tagging a NixOS release candidate, evaluate and build the flake outputs
and execute the ordered steps in [NIXOS_QUALIFICATION](NIXOS_QUALIFICATION.md).
