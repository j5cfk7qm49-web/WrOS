# WrOS release checklist

## Build and qualify the candidate

1. Record the candidate commit and verify that `os/VERSION`, the RC contract
   and the packaging test agree. Run the Rust workspace and Python suites in
   the Debian 13 chroot. Keep the full logs and identify each failed test.
2. From WSL, run the versioned `rc-x-rc7-release.sh` helper. It fetches the
   local `rc7-build` commit into the release checkout, builds the package in
   Debian 13 and signs the bundle with the offline release key. Keep the
   signing directory outside the source tree. The helper removes its private
   copy of the Google client file on exit.
3. Run `rc-x-rc7-checkbundle.sh` on `rc7-release/wros-updates.tar`. Confirm the
   version, the pinned signer and the tarball SHA-256. Keep the release notes
   and GitHub release body beside the tarball.
4. Run `rc7-vm-chain.sh` from WSL with the bundle and `--expect 0.1.0~rc7`.
   The chain stages the update from an encrypted rc1 VM through Settings,
   installs it at restart and checks the installed features. Run the rc6
   disk migration as well. Record the run directories and inspect each
   `result.json`. Check the old settings, Colemak, group IDs, recovery menu,
   manual go-back evidence, and lock and suspend password isolation.
5. Review the VM results and hardware checklist before approving the tag.
   Release tags use the checked `wros-updates.tar` as their live payload.

The local layer-two checks use the actual editor in a private pseudo-terminal.
In the Debian chroot, build with `cargo build --offline --locked
--no-default-features --features pty-qa -p grimoire-tui`. Set `GRIMOIRE_BINARY`
to that debug executable and run `WROS_QA_PTY=1 PYTHONPATH=os/tests python3 -m
unittest test_rc7_pty`. The fixture locker tests input retirement and typeahead
isolation. The QEMU feature phase tests the installed locker and PAM.

## HP Stream hardware checks

Run [HARDWARE_CHECKLIST](HARDWARE_CHECKLIST.md) on the laptop: typing speed,
lid, lock and sleep, Wi-Fi after sleep and over a day, the school network, low
battery, shutdown during a backup, the recovery boot, and what an update must
carry over. Record the results in its table.

## Verify a live update after publication

1. Check that the Pages deployment succeeded and that its `updates/` contains
   `InRelease`, `Release`, `Release.gpg`, `Packages` and the release package.
   Run `python3 -I tools/publish-updates.py published <updates-url> <empty-dir>`
   in a fresh local directory. Compare the downloaded package SHA-256 with the
   offline bundle.
2. On a disposable older WrOS installation, open **Settings › System › Updates**
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
