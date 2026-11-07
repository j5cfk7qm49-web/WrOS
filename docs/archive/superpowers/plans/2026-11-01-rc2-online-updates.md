# rc2: Online WrOS Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship rc2 — this session's fixes, network time, and the client side of online WrOS updates — to the rc1 laptop without a USB stick, so every later update arrives from GitHub.

**Architecture:** GitHub Pages serves the existing signed update-bundle format, which is also a flat `apt` repository. The laptop learns a CI signing subkey through rc2's package-owned keyring while the image-owned pin keeps trusting only the primary. A login check reads signed metadata; Update WrOS downloads into a root-owned cache and hands it to the unchanged review → stage → apply-at-restart flow.

**Tech Stack:** Python 3 (engine, helpers, packaging), Rust (grimoire-tui, writingdeck-sys), GnuPG/gpgv, GitHub Actions, Debian trixie.

**Spec:** `docs/superpowers/specs/2026-11-01-online-wros-updates-design.md`; network time and Debian sources from `docs/superpowers/plans/2026-11-01-debian-sources-and-network-time.md`.

## Global Constraints

- Update URL, fixed in the image, never user-supplied: `https://j5cfk7qm49-web.github.io/WrOS/updates/`
- Bundle: `Origin: WrOS`, `Suite: wros-updates`, `Valid-Until` ≤ 30 days after `Date`; CI re-signs weekly with 21-day validity.
- Pin file `/usr/share/wros/update-signers` stays image-owned (`install-trust`). Keyring `/usr/share/keyrings/wros-archive-keyring.gpg` becomes `wros`-package-owned and may hold **no primary key except the pinned one** — refused by the package build, the engine and the media audit.
- CI holds only the signing subkey (`WROS_UPDATE_SIGNING_SUBKEY`, `WROS_UPDATE_SIGNING_PASSPHRASE`); the primary secret never leaves this PC.
- rc2 (`0.1.0~rc2`) is signed locally with the primary and delivered via SSH + `/mnt/wros-rc2`; it is never published online. CI publishes from rc3 (`v0.1.0-rc3` → `0.1.0~rc3`, must equal `os/VERSION`).
- Online updates carry only the `wros` package; `Packages` indexes only the newest `wros`.
- Login check: once after login and at most daily, 10 s timeout, fetches only `InRelease` and `Packages`; silent on failure except the Settings › Updates line.
- Clock-failure copy: `Couldn't check for WrOS updates: this computer's clock may be wrong (it reads <YYYY-MM-DD HH:MM>).`
- Alert copy: `WrOS <version> is available · Settings › Updates`.
- The laptop has no `gpg`, only `gpgv`: engine code may not call `gpg`.
- Commit only with the writer's go-ahead (one approval covers this plan's per-task commits).

## Review Focus

1. Clock wrong (laptop is rc1 with NTP off): the check must report the clock line, never a scary signature error, and never alert.
2. Online bundle older than or equal to the installed `wros` (e.g. after `apt upgrade` installed it): no alert, and Update WrOS refuses as already current.
3. A keyring containing a second primary key (malicious or mistaken package): build, engine and audit all refuse.
4. Pages partly published (InRelease new, Packages old) or a `.deb` 404: the fetch fails closed with the cache folder removed and nothing reviewable left behind.
5. Offline at login, or GitHub slow: no alert, no hang — the 10 s timeout and the editor stays responsive (the check runs on the system worker thread).

---

### Task 0: Commit this session's verified fixes

Already implemented and tested in the working tree: Google token from a file, backup console fix and progress, menu row kinds, Google connection failure kinds, queued-trigger repair check, backlight rule, and the two design docs and the earlier Debian-sources plan.

- [ ] **Step 1:** Re-run the full suites once (Python incl. backup PTY with `GRIMOIRE_BINARY`, Rust, fmt, clippy); expected all pass.
- [ ] **Step 2:** Commit them on `development` as separate commits by topic (one per fix), with the writer's go-ahead.

### Task 1: Network time and Debian sources

Carry out Tasks 1–3 of `docs/superpowers/plans/2026-11-01-debian-sources-and-network-time.md` as written. New images get `systemd-timesyncd` from the image package list; the `wros` package does **not** depend on it. The rc1 laptop installs it with `apt` (the writer has Debian sources), so rc2's bundle carries only `wros`.

- [ ] **Step 1:** Execute that plan's Tasks 1–3 (tests first, as written there).
- [ ] **Step 2:** Run `test_native_image test_target_hygiene test_packaging` and the Rust Date-and-time test; expected all pass.
- [ ] **Step 3:** Commit `feat: network time and Debian sources on new installs`.

### Task 2: Package-owned keyring, image-owned pin

**Files:**
- Create: `os/config/update-trust/wros-archive-keyring.gpg` (public, from Task 3), `os/config/update-trust/update-signers`
- Modify: `packaging/manifest.json`, `packaging/wros_packaging.py:446-530`, `os/scripts/install-files.sh:84-86`, `os/scripts/wros-update-engine.py` (new parser + check in `verify_signed_metadata`), `tools/audit-installer-media.py`
- Test: `os/tests/test_packaging.py`, `os/tests/test_update.py`, `os/tests/test_native_image.py`

**Interfaces:**
- Produces: `keyring_primary_fingerprints(data: bytes) -> list[str]` in the engine (pure Python, OpenPGP v4 public-key packets, uppercase 40-hex; `fail()` on any other key version or malformed packet). `verify_signed_metadata` calls it on the keyring and fails `'update keyring holds a primary key the signer pin does not name'` unless the set equals the pinned set.
- `install_trust(root, environ, fixtures)` installs **only** the pin; the keyring arrives in the package. `check_trust_material` is reused by the package build against the committed pin and by `install-trust` against the image's `WROS_UPDATE_KEYRING`/`WROS_UPDATE_SIGNERS` (they must agree with the package's keyring primaries).

- [ ] **Step 1: Failing tests.**
  - `test_update.py::test_keyring_primaries_must_equal_the_pin` — a fixture keyring with the pinned primary plus a subkey verifies a subkey-signed bundle; adding a second primary fails with the message above; `keyring_primary_fingerprints` matches `gpg --show-keys --with-colons` fingerprints for the fixture.
  - `test_packaging.py::test_keyring_is_package_owned_and_pin_is_not` — manifest has `usr/share/keyrings/wros-archive-keyring.gpg` (0644, class `trust-public`), never `usr/share/wros/update-signers`; building with a keyring holding an extra primary raises `PackagingError`.
- [ ] **Step 2:** Run them; expect FAIL (no parser, keyring absent from manifest).
- [ ] **Step 3:** Implement as the interfaces say; update the `install-files.sh` comment to the new rule ("the pin is never package-owned; the keyring may be, with no primary but the pinned one").
- [ ] **Step 4:** Media audit: the image's keyring primaries equal its pin; existing audit tests adjusted, one new failing-then-passing case for an extra primary.
- [ ] **Step 5:** Run `test_update test_packaging test_native_image`; expected all pass.
- [ ] **Step 6:** Commit `feat: package-owned update keyring with an image-owned pin`.

### Task 3: Signing subkey (maintainer PC, writer's GitHub)

Work in WSL against `/home/aegeorge/rc-keys/gnupg`; nothing secret enters the repository or the chroot.

- [ ] **Step 1:** The writer runs, in their own terminal on this PC (pinentry asks them for a new passphrase; it is never typed by an agent): `gpg --homedir /home/aegeorge/rc-keys/gnupg --quick-add-key <primary-fpr> ed25519 sign 2y`.
- [ ] **Step 2:** Export the public key (primary + subkey) to `os/config/update-trust/wros-archive-keyring.gpg`; write the primary fingerprint to `update-signers`. Verify: `gpg --show-keys` lists exactly one `pub` (the pin) and one `sub` with usage `S`.
- [ ] **Step 3:** `gpg --export-secret-subkeys <subkey-fpr>!` to a file in the scratchpad; confirm with `gpg --list-packets` that the primary is a stub (`gnu-dummy`).
- [ ] **Step 4:** The writer runs, themselves: `gh secret set WROS_UPDATE_SIGNING_SUBKEY < <file>` and `gh secret set WROS_UPDATE_SIGNING_PASSPHRASE`. Then the exported file is shredded.
- [ ] **Step 5:** Commit the public keyring and pin `chore: update keyring with CI signing subkey`.

### Task 4: Engine — online check and fetch

**Files:**
- Modify: `os/scripts/wros-update-engine.py` (constants, `check_online`, `fetch_online`, `installed_version`, `ALLOWED_MEDIA_PREFIXES`, `main`), `crates/writingdeck-sys/src/system_views.rs:523` (accept the cache prefix)
- Create: `os/scripts/wros-update-fetch` (pkexec launcher modelled on `wros-update-media`), polkit action `org.wros.update.fetch` in `os/config/org.wros.update.policy` and `50-wros-update.rules`, manifest entries
- Test: `os/tests/test_update.py`, `os/tests/test_update_support.py`

**Interfaces:**
- `ONLINE_URL = 'https://j5cfk7qm49-web.github.io/WrOS/updates/'`; `ONLINE_CACHE = Path('/var/cache/wros-update/online')`; `ALLOWED_MEDIA_PREFIXES` gains `'/var/cache/wros-update/online/'`.
- `installed_version(dpkg_dir, package='wros') -> str | None` from the dpkg status file; used by review/stage anti-downgrade together with the engine's last-applied record (the higher wins).
- `check_online(*, base_url=ONLINE_URL, now=None, dpkg_dir=..., keyring=..., signers=..., opener=None) -> dict` — `{'status': 'available'|'current'|'clock'|'unreachable'|'invalid', 'version': str|None, 'installed': str|None, 'clock': 'YYYY-MM-DD HH:MM'}`. Fetches `InRelease` (≤1 MiB) and `Packages` (≤4 MiB) with a 10 s timeout; `clock` when verification fails only on `Date`-in-future or expired `Valid-Until`.
- `fetch_online(*, base_url=ONLINE_URL, cache=ONLINE_CACHE, ...) -> dict` — `{'path': '/var/cache/wros-update/online/<sha256(InRelease)[:16]>'}`; downloads into a `.partial` sibling, checks every `.deb` size and SHA256 against `Packages`, runs `verify_bundle`, then renames. Any failure removes the partial and raises.
- Verbs: `wros-update check-online` (unprivileged, JSON on stdout), `wros-update-fetch` (pkexec, root, JSON). `opener`/`base_url` are function parameters for tests only, never environment.

- [ ] **Step 1: Failing tests** in `test_update.py` with a `http.server` thread serving a fixture-signed bundle:
  - `test_check_online_reports_available_current_clock_and_unreachable` — newer → `available`; equal or older → `current`; `now` 40 days on → `clock`; closed port → `unreachable`; tampered `Packages` → `invalid`.
  - `test_fetch_online_publishes_only_a_verified_bundle` — success yields a reviewable path under the cache; a 404 `.deb`, a wrong SHA256, or InRelease/Packages mismatch leaves no folder.
  - `test_installed_dpkg_version_counts_for_anti_downgrade` — a bundle equal to the dpkg-installed `wros` is refused as not newer.
- [ ] **Step 2:** Run; expect FAIL.
- [ ] **Step 3:** Implement per the interfaces (urllib, explicit size caps, no redirects off the host).
- [ ] **Step 4:** Rust path validation accepts the cache prefix; extend its existing test.
- [ ] **Step 5:** Run `test_update test_update_support test_packaging` and `cargo test -p writingdeck-sys`; expected all pass.
- [ ] **Step 6:** Commit `feat: online WrOS update check and verified fetch`.

### Task 5: App — login alert and Update WrOS

**Files:**
- Modify: `crates/writingdeck-sys/src/lib.rs` (`Request::CheckOnlineUpdates`, `Request::FetchOnlineUpdate`, `Reply::OnlineUpdate(OnlineUpdate)`, `Reply::UpdateMedia` reused for the fetched path), `crates/writingdeck-sys/src/linux.rs` (invocations), `src/app/shell/update_flow.rs`, `src/app/shell.rs` (Updates panel rows), `src/app/writingdeck.rs` (login check scheduling)
- Test: `src/shell_ui_tests.rs`, `crates/writingdeck-sys` tests

**Interfaces:**
- `pub struct OnlineUpdate { pub status: String, pub version: Option<String>, pub installed: Option<String>, pub clock: String }`
- Check runs once after an installed (non-live) session starts and again when 24 h have passed while running, via the existing system worker; never while another system request is busy.
- Settings › Updates gains, first: a description row (`WrOS 0.1.0~rc3 is available` / `WrOS is up to date (0.1.0~rc2)` / the clock line / `Couldn't reach the WrOS update server`), then the option **Update WrOS to <version>** (only when available), which runs `FetchOnlineUpdate` and on success opens the existing review screen on the returned path.
- `available` pushes the alert copy as a notice once per version per session.

- [ ] **Step 1: Failing UI tests:** `online_update_available_alerts_once_and_update_opens_the_existing_review`, `online_update_clock_and_offline_states_never_alert` (simulated backend replies).
- [ ] **Step 2:** Run; expect FAIL.
- [ ] **Step 3:** Implement; simulated backend gets the two requests.
- [ ] **Step 4:** Run the full Rust suite, rustfmt, clippy; expected pass.
- [ ] **Step 5:** Commit `feat: login alert and Update WrOS from GitHub`.

### Task 6: apt source, CI publishing, docs

**Files:**
- Create: `os/config/apt/wros.sources` (`Types: deb`, `URIs:` the update URL, `Suites: ./`, `Signed-By: /usr/share/keyrings/wros-archive-keyring.gpg`), `.github/workflows/release-updates.yml`
- Modify: `packaging/manifest.json`, `docs/API_AND_NETWORK_USE.md`, `docs/RC_CONTRACT.md`, the update docs that describe the USB route
- Test: `os/tests/test_packaging.py`, `os/tests/test_update_workflow.py` (new: parses the workflow YAML)

- [ ] **Step 1: Failing tests:** `wros.sources` in the manifest at `etc/apt/sources.list.d/wros.sources` with exactly those fields; the workflow triggers on `v*` tags, a weekly schedule and `workflow_dispatch` (input `scratch`), checks tag == `os/VERSION`, imports only `WROS_UPDATE_SIGNING_SUBKEY`, calls `tools/build-update-bundle.py --valid-days 21 --expect-fingerprint` with the committed pin, and deploys to Pages (scratch input → `updates-scratch/`).
- [ ] **Step 2:** Run; expect FAIL.
- [ ] **Step 3:** Write the source file, the workflow (Debian 13 container as in `ci.yml`; `cargo build --release --locked --no-default-features -p grimoire-tui`; `tools/build-wros-debs.py`; weekly job re-signs the current `.deb`), and the docs.
- [ ] **Step 4:** Run tests; expected pass. `actionlint` if available.
- [ ] **Step 5:** Commit `feat: publish signed WrOS updates to GitHub Pages`.

### Task 7: Build rc2 and install it on the laptop

- [ ] **Step 1:** Set `os/VERSION` to `0.1.0~rc2`; run every suite (Python incl. PTY with `GRIMOIRE_BINARY`, Rust, fmt, clippy). Commit `release: 0.1.0~rc2`; push `development` with the writer's go-ahead.
- [ ] **Step 2:** In the chroot: release-build `grimoire`, `tools/build-wros-debs.py --version 0.1.0~rc2`.
- [ ] **Step 3:** `tools/build-update-bundle.py <wros.deb> --gnupg-home rc-keys --key-id <primary> --expect-fingerprint <primary> --valid-days 14`, signed with the **primary**; it self-verifies with the real engine.
- [ ] **Step 4:** Verify against rc1's real trust: copy the bundle to the laptop's `~/wros-rc2` and run rc1's `/usr/libexec/wros-update verify-bundle ~/wros-rc2` over SSH (review refuses home paths by policy); expect success. The writer's review in Step 5 lists `wros 0.1.0~rc1 → 0.1.0~rc2`.
- [ ] **Step 5:** `scp` the bundle to `~/wros-rc2`. The writer runs `sudo cp -r ~/wros-rc2 /mnt/`, then Settings › Updates › Review signed offline update → `/mnt/wros-rc2`, confirms, makes the backup it asks for, and restarts.
- [ ] **Step 6:** Over SSH: `dpkg-query -W wros systemd-timesyncd` shows rc2 and the writer's apt-installed timesyncd; `timedatectl show -p NTP -p NTPSynchronized` → `NTP=yes`; `/etc/apt/sources.list.d/wros.sources` present; keyring lists the subkey; `wros-update check-online` returns `unreachable` or `current` (nothing published yet) — never `invalid`. Then the writer starts a Drive backup on tty1 and presses keys: no logout.

### Task 8: First online publish (rc3)

Runs when rc3 (plain-text Drive sync) is ready; recorded here so the pipeline is proven end to end.

- [ ] **Step 0 (from the rc2 review):** scheduled workflows run only from the default branch, and the `github-pages` environment deploys only from it by default. With the writer's go-ahead: merge `development` (with `release-updates.yml`) into `main`; in the `github-pages` environment allow deployments from `v*` tags; load the subkey secret (Task 3 Step 4). GitHub disables schedules after 60 days without repository activity: the laptop then shows "the WrOS update server's signature has expired", and re-enabling the workflow (or any push) resumes re-signing.
- [ ] **Step 1:** With the writer's go-ahead, enable GitHub Pages (Actions source) and run the workflow by hand with `scratch: true`; confirm `updates-scratch/InRelease` verifies with the committed keyring.
- [ ] **Step 2:** Tag `v0.1.0-rc3`; confirm Pages serves it, the laptop alerts at login, and Update WrOS installs at restart.
