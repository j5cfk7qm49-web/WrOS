# WrOS release checklist

## Build and qualify the candidate

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

## Check installation without Internet

Use the audited ISO in the Debian builder with QEMU, `iproute2`, and `tcpdump`.
Run the full install with a wired virtual NIC inside a fresh Linux network
namespace. The guest gets a DHCP route and DNS address, while the host namespace
has no active links to the Internet. The installation must finish using the
media when online sources cannot be reached. Keep the IPv4 capture as a record
of attempted network access; DNS, network-time and update requests are allowed.

Network time and update checks stay enabled by default. An online installation
may retrieve missing drivers or packages. The offline image still carries the
base packages and firmware for its supported hardware; additional hardware can
use Debian's network-assisted setup when a connection is available.

```sh
sudo unshare --net runuser -u builder -- python3 os/tests/rc_vm_phases.py \
  install --iso "$iso" --layout colemak --audit-network
```

Keep the result and the private `install.pcap`. The installer VM uses its own
disposable disk. This check covers IPv4 during a wired installation. Verify
Wi-Fi provisioning separately on the laptop.

## HP Stream hardware checks

Run [HARDWARE_CHECKLIST](HARDWARE_CHECKLIST.md) on the laptop: typing speed,
lid, lock and sleep, Wi-Fi after sleep and over a day, the school network, low
battery, shutdown during a backup, the recovery boot, and what an update must
carry over. Record the results in its table.

## Verify a live update after publication

1. Check that the Pages deployment succeeded and that its `updates/` contains
   `InRelease`, `Release`, `Release.gpg`, `Packages` and the bundle's packages.
   Run `python3 -I tools/publish-updates.py published <updates-url> <empty-dir>`
   in a fresh local directory. Compare each downloaded package's SHA-256 with
   its entry in the offline bundle.
2. On a disposable older WrOS installation, open **Settings › Updates and recovery › Updates**
   and choose **Check for WrOS updates**. Review the version, signer and validity
   dates. Confirm staging, restart, unlock and check that Settings reports the
   completed update and that the original book is intact.
3. On the HP Stream, confirm the recovery menu is reachable with Esc at boot.
   Try **Show what happened** and **Continue to the normal login**.

## Set up the weekly signing subkey

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
5. After the first weekly run, verify that the `InRelease` date advanced and
   the package checksum stayed the same. The workflow verifies the old
   published package before re-signing it for 21 days. GitHub runs the schedule
   from the default branch and may disable it after 60 days of inactivity.
   [GitHub schedule behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)
