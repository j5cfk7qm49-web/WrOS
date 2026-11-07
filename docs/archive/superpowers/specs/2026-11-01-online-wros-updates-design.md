# Online WrOS updates from GitHub — design

## Intent

Today a WrOS update reaches a computer only on a USB stick labelled
`WROS-UPDATE`, signed by hand on the maintainer's PC. The writer wants WrOS
updates to behave like a normal update: GitHub hosts them, they are signed
automatically with no manual step, an alert at login says a WrOS update is
available, and a place in Settings installs it. The Debian system underneath
keeps updating with `apt` as usual; alerts are for WrOS updates.

## Goals

- Publishing: pushing a version tag (`v0.1.0-rc2`) on GitHub builds the `wros`
  package, signs the update metadata and publishes it. No manual signing step.
- Hosting: GitHub Pages serves one folder that is both a valid flat `apt`
  repository and an update the existing WrOS update engine accepts.
- Alert: after login, WrOS checks once in the background; if a newer WrOS is
  published and signed, it shows "WrOS <version> is available · Settings ›
  Updates". Offline or on any failure, it says nothing.
- Install: Settings › Updates › **Update WrOS** downloads that update and hands
  it to the existing review → stage → apply-at-restart flow (signature check,
  list of changes, fresh-backup gate, install at the next restart, repair and
  rescue unchanged).
- `apt` also sees the repository, so `sudo apt update && sudo apt upgrade` can
  update WrOS as well as Debian.

## Non-goals

- Alerts or a menu for Debian package updates (they stay with `apt`, per
  `docs/superpowers/plans/2026-11-01-debian-sources-and-network-time.md`).
- Automatic installation without the writer choosing Update WrOS.
- WrOS updates that need a Debian package the computer does not already have.
- Replacing the USB route: offline updates keep working exactly as now.

## Anti-goals

- A compromised GitHub account or CI run able to make computers trust a new
  root: computers pin the **primary** key fingerprint; CI only ever holds a
  signing **subkey**.
- Installing, downloading packages or nagging without the writer's choice; the
  login check fetches only signed metadata.
- An update changing the system under an open book (apply stays at restart).
- Any user-supplied URL: the repository address is fixed in the image.
- Weakening an existing engine check (signature, Origin/Suite pin, Valid-Until,
  anti-downgrade, checksums, path rules).

## Constraints

- The engine's bundle format (`tools/build-update-bundle.py`): `InRelease`,
  `Release`, `Release.gpg`, `Packages`, `pool/*.deb`; `Origin: WrOS`,
  `Suite: wros-updates`, `Valid-Until` at most **30 days** after `Date` and not
  passed. This layout is served unchanged as a flat `apt` repository
  (`Suites: ./`).
- Signing root: the existing update signer whose primary fingerprint the rc1
  image pins in `/usr/share/wros/update-signers`, with its public key in
  `/usr/share/keyrings/wros-archive-keyring.gpg`. The rc1 laptop already trusts
  that primary; it learns the new subkey from the rc2 offline update.
- Tag to version: `v0.1.0-rc3` builds `0.1.0~rc3` and fails unless it matches
  `os/VERSION`. `Packages` lists only the newest `wros`; re-signing re-runs
  `tools/build-update-bundle.py` on the published `.deb`.
- The GitHub repository `j5cfk7qm49-web/WrOS` is public; Pages is not yet
  enabled.
- The CI-built `wros` package must contain the same files as the release
  pipeline's (`tools/build-wros-debs.py` from the tagged commit), including
  whatever runtime assets the release build adds.
- `docs/API_AND_NETWORK_USE.md` must list the new background check: what it
  contacts, when, and what it sends (nothing beyond the HTTPS request).
- The laptop gets this feature through one more offline update (rc2, copied
  over SSH, no USB stick); every later update (rc3 on, including plain-text
  Drive sync) can arrive online.

## Decided how

**Keys.** On the maintainer's PC, add a signing subkey to the existing update
signer key and export **only that subkey** (`gpg --export-secret-subkeys`) to
the repository secret `WROS_UPDATE_SIGNING_SUBKEY` (with its passphrase in
`WROS_UPDATE_SIGNING_PASSPHRASE`). The primary secret key never leaves the PC
and should then be moved to offline storage. *(My call: reuse the pinned key
rather than a new root, which would need its own offline re-keying update.)*

**As built (ruling during rc2):** the image keyring and pin stay image-owned
exactly as before. The `wros` package adds a second keyring,
`/usr/share/wros/update-keyring.gpg` (primary + signing subkey); the engine
passes it to `gpgv` only after refusing it if any primary in it is unpinned,
the package build refuses it unless its primaries equal the committed pin, and
the image audit refuses it unless the image pin names its primaries. `apt`'s
`Signed-By` uses the package keyring. The paragraph below is the original plan.

**Trust anchor rule, changed deliberately.** *(My call, flagged to the
writer.)* `gpgv` can only check a subkey signature if the subkey's public half
is in `/usr/share/keyrings/wros-archive-keyring.gpg`, and rc1's keyring
predates the subkey. Today `install-files.sh`, `wros_packaging.py
install-trust`, the manifest and the media audit forbid any package from owning
trust files. The new rule: the **pin file** `/usr/share/wros/update-signers`
(the primary fingerprint) stays image-owned and never package-owned; the
**keyring** may be updated by a `wros` package, but the package build, the
engine and the media audit all refuse a keyring containing any primary key
other than the pinned one. This is safe because a subkey verifies only if the
pinned primary signed its binding, and a substituted primary fails the pin.
Each of those four places, their tests and `docs/` change together.

**First releases.** *(My call.)* rc2 is built and signed on the maintainer's
PC with the primary key, as today. It needs no USB stick: it is copied over
SSH to the writer's home, the writer copies it into `/mnt/wros-rc2` with one
`sudo cp`, and Settings › Updates reviews that folder (the engine accepts any
non-symlink folder under `/mnt/`). It carries the
keyring with the new subkey, `wros.sources`, the login check and Update WrOS.
It is **not** published online. CI publishes from **rc3** on, so the same
version never exists as two different files.

**What updates online.** *(My call.)* Only the `wros` package: `grimoire`,
`wros-display-hw`, the generated keymaps and the manifest's files. The prose
runtime, `wros-greeter` and `wros-installer` are staged outside the package by
the image build and keep updating only through new images or offline updates. CI
therefore needs `cargo build --release -p grimoire-tui` and
`tools/build-wros-debs.py`, not the prose-runtime preparation.

**Clock and Debian sources.** *(My call.)* rc2 also carries the Debian sources
and network time plan
(`docs/superpowers/plans/2026-11-01-debian-sources-and-network-time.md`),
so the clock is set from the network before the first online check. If the
check still fails on time (a `Date` in the future or an expired
`Valid-Until`), the alert stays silent and Settings › Updates says "Couldn't
check for WrOS updates: this computer's clock may be wrong", with the time it
reads. `API_AND_NETWORK_USE.md` lists the login check and that Debian's
`apt-daily` timers will now contact Debian and the Pages URL.

**Publishing workflow** (`.github/workflows/release-updates.yml`):

1. On a `v*` tag: in a Debian trixie container, build the release binaries and
   the `wros` package exactly as the release pipeline does, then run
   `tools/build-update-bundle.py` with the imported subkey. It already verifies
   the result with the real engine and refuses a key whose primary fingerprint
   is not the pinned one.
2. Publish the bundle folder to GitHub Pages at
   `https://j5cfk7qm49-web.github.io/WrOS/updates/`, with only the newest `wros`
   package indexed.
3. Weekly (schedule) and on demand: re-sign the current `Release` with a fresh
   `Date` and `Valid-Until` of **21 days**, so a working CI never lets it lapse.
   If CI stops, updates fail closed after 21 days with "the update signature
   has expired" and the alert stays silent.

**On the computer.**

- `/etc/apt/sources.list.d/wros.sources`: `Types: deb`, `URIs:` the Pages URL,
  `Suites: ./`, `Signed-By: /usr/share/keyrings/wros-archive-keyring.gpg`.
  Shipped in the `wros` package and installed on new systems.
- **Login check:** after login, and at most once a day while running, the
  unprivileged app fetches `InRelease` and `Packages` (no `.deb`), verifies them
  with the engine's own `verify_signed_metadata` and Packages parser, and
  compares the `wros` version with the installed one (from dpkg). Newer and
  signed shows the alert as a notice and a line in Settings › Updates. A 10 s
  timeout; no retries until the next login or day.
- **Update WrOS:** a new root-owned helper, reached through polkit like the
  other update helpers, downloads the fixed-URL bundle into
  `/var/cache/wros-update/online/<id>/` (root-owned, not writable by the
  writer) and verifies it. That directory becomes one more accepted review
  location, and the existing review screen opens on it. From there, nothing
  changes.
- **Version truth:** because `apt` can also upgrade `wros`, the engine treats
  the dpkg-installed version as the installed version for anti-downgrade and
  "newer" decisions, not only its own last-applied record.

## Left to the builder

- Container image and caching for the CI build; how the release assets are
  produced in CI.
- Pages deployment mechanism (a `gh-pages` branch or the Pages artifact action).
- The download helper's exact name, polkit action and progress display.
- Wording beyond the strings above, and where the alert appears besides the
  notice line.
- Tests: an engine test against a local HTTP server serving a signed bundle,
  a PTY test of the alert and Update WrOS flow, and a CI dry run on a test tag
  that publishes to a scratch Pages path.

## Not proven by tests

The first real publish from a tag, Pages serving it, and the rc1 laptop (after
the rc2 offline update) seeing the alert and installing rc3 online need a live run.
