# Debian Online Sources and Network Time Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A newly installed WrOS has Debian's online package sources (so `apt` gets Debian security fixes) and keeps its clock right from the network, without changing how an offline install or a signed WrOS update works.

**Architecture:** The image ships a sources template and `systemd-timesyncd`. The installer's existing target-hygiene hook (`93wros-target-hygiene`, after Debian's only finish-time `apt-get update` in `10apt-cdrom-setup`) copies the template into the target, and the target proof (`94wros-target-proof`) requires it. Settings shows network-time state. Applying Debian updates through WrOS's review/offline-apply flow is a separate, later plan; until then the docs say `sudo apt update && sudo apt upgrade`.

**Tech Stack:** Debian 13 live-build image, Python 3 installer session (`os/scripts/wros-install-session`), Rust (`crates/writingdeck-sys`, `src/app/shell.rs`), Python unittest, cargo test, the RC VM harness (`os/tests/rc_vm_phases.py`).

**Spec:** this conversation's decision (2026-11-01): "installs set up Debian's online sources by default" and "I'd rather have the time set by network". Facts it rests on: installer preseed sets `apt-setup/use_mirror false` and `clock-setup/ntp false`; the installed target is a copy of the live image; `systemd-timesyncd` is not in the image or on the medium; the update engine plans with `Dir::Etc::sourcelist=/dev/null`, `sourceparts=-` and a private empty `Dir::State::lists` (`os/scripts/wros-update-engine.py:947-948`).

## Global Constraints

- Template path in the image: `/usr/share/wros/apt/debian.sources`, mode 0644, root:root.
- Installed path: `/etc/apt/sources.list.d/debian.sources`, mode 0644, root:root, byte-identical to the template.
- Template content, exactly:
  ```
  Types: deb
  URIs: http://deb.debian.org/debian
  Suites: trixie trixie-updates
  Components: main non-free-firmware
  Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp

  Types: deb
  URIs: http://security.debian.org/debian-security
  Suites: trixie-security
  Components: main non-free-firmware
  Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp
  ```
  (`non-free-firmware` matches the image's `--archive-areas`.)
- The installer stays fully offline: nothing may add an online source to the target before hook 93.
- WrOS signed updates stay unaffected: the engine's private sources/lists options are not touched.
- No new privileged operation; Settings only reads network-time state.
- NetworkManager's connectivity check stays disabled (`60-wros-no-connectivity-check.conf`).

## Review Focus

1. **Offline installation with the new hook** — an install with no network must still complete; a failed copy must fail the install with the hook's marker, not leave a half-configured target. Pinned by Task 2's hygiene tests and Task 4's offline VM install.
2. **Template missing or altered in the image** — the target must not get a stale or edited file silently. Pinned by Task 1's audit test and Task 2's refusal test.
3. **WrOS update with Debian sources present** — review/stage/offline apply must behave exactly as before. Pinned by Task 4's update `ok` variant on an install that has the file.
4. **Clock far off at first boot** — timesyncd must still step it; `apt-daily` may fail until then and retry. Pinned by Task 4's installed-boot check that the NTP service is enabled; recorded, not asserted, offline.
5. **Settings when `timedatectl show` fails or prints unknown values** — the row must say `unknown`, not crash or claim sync. Pinned by Task 3's parser test.

---

### Task 1: Ship the sources template and network time in the image

**Files:**
- Create: `os/config/apt/debian.sources` (content from Global Constraints)
- Modify: `os/config/packages.txt` (add `systemd-timesyncd`)
- Modify: `os/scripts/install-files.sh` (install the template to `usr/share/wros/apt/debian.sources`, 0644)
- Modify: `tools/audit-native-image.py` (require the template, byte-identical to the repo file; require `systemd-timesyncd` installed and `debian-archive-keyring.pgp` present)
- Test: `os/tests/test_native_image.py`

**Interfaces:**
- Produces: `/usr/share/wros/apt/debian.sources` in every image; audit constant `DEBIAN_SOURCES_TEMPLATE = 'usr/share/wros/apt/debian.sources'`.

- [ ] **Step 1: Write the failing test** `test_debian_sources_template_and_network_time_are_required` in `os/tests/test_native_image.py`: on the existing fixture root, the audit passes with the template (copied from `os/config/apt/debian.sources`), `var/lib/dpkg/info/systemd-timesyncd.list` and `usr/share/keyrings/debian-archive-keyring.pgp` present; it raises `ValueError` matching `debian.sources` when the template is missing or one byte differs, and matching `systemd-timesyncd` when that package is absent.
- [ ] **Step 2: Run it to verify it fails** — `python3 -m unittest test_native_image -k debian_sources` (from `os/tests`). Expected: FAIL.
- [ ] **Step 3: Implement** the template file, the package-list line, the `install -m 0644` in `install-files.sh` beside the other `usr/share/wros` files, and the audit checks.
- [ ] **Step 4: Run** `python3 -m unittest test_native_image test_installer_media test_live_session` — expected: OK.
- [ ] **Step 5: Commit** — `git commit -m "Ship Debian's sources template and systemd-timesyncd in the image"`.

### Task 2: Install the sources on the target and prove them

**Files:**
- Modify: `os/scripts/wros-install-session` (new `install_debian_sources(root=Path('/'))`, called from `in_target_hygiene` after `purge_live_artifacts`; `verify_target_hygiene` also requires it)
- Test: `os/tests/test_install_session.py`

**Interfaces:**
- Consumes: Task 1's template path.
- Produces: `install_debian_sources(root: Path = Path('/')) -> None` — copies `root/usr/share/wros/apt/debian.sources` to `root/etc/apt/sources.list.d/debian.sources` atomically (temp file in the same directory, `os.replace`), 0644; raises `ValueError('Debian sources template missing or not a regular file')` when the template is absent, a symlink or not regular. `verify_target_hygiene` raises `ValueError('Debian sources not installed on the target')` unless the installed file is a regular (not symlinked) file with mode 0644 and bytes equal to the template. Ownership is not checked here: the hook runs as root in the target, and the unit tests run unprivileged.

- [ ] **Step 1: Write the failing tests** in `os/tests/test_install_session.py`:
  - `test_hygiene_installs_debian_sources_from_the_image_template`: on a temp root with the template, `install_debian_sources(root)` leaves `etc/apt/sources.list.d/debian.sources` byte-equal to the template with mode `0o644`; running it twice is a no-op; `verify_target_hygiene` accepts the result (with the existing fixture's other hygiene inputs).
  - `test_hygiene_refuses_a_missing_or_symlinked_template`: absent template and a symlinked template both raise `ValueError` matching `template`; no file appears under `etc/apt/sources.list.d`.
  - `test_target_proof_rejects_missing_or_edited_sources`: `verify_target_hygiene` raises matching `Debian sources` when the installed file is absent or differs from the template.
- [ ] **Step 2: Run them to verify they fail** — `python3 -m unittest test_install_session -k sources`. Expected: FAIL (`install_debian_sources` undefined).
- [ ] **Step 3: Implement** `install_debian_sources` and the proof check; call it in `in_target_hygiene` after the purge.
- [ ] **Step 4: Run** `python3 -m unittest test_install_session` — expected: OK.
- [ ] **Step 5: Commit** — `git commit -m "Install Debian's online sources on the target after the offline install"`.

### Task 3: Show network time in Settings

**Files:**
- Modify: `crates/writingdeck-sys/src/lib.rs` (`Snapshot`: `pub network_time: Option<bool>`, `pub time_synchronized: Option<bool>`; `Simulated::laptop()` sets `Some(true)`, `Some(false)`)
- Modify: `crates/writingdeck-sys/src/linux.rs` (beside the timezone read at line 209: run `timedatectl --no-pager show --property=NTP --property=NTPSynchronized`; new `fn parse_time_sync(text: &str) -> (Option<bool>, Option<bool>)`)
- Modify: `src/app/shell.rs:540-549` (replace the row "Manual clock / network time controls are not available yet")
- Modify: `docs/NATIVE_TIMEZONE.md`, `docs/API_AND_NETWORK_USE.md`
- Test: `crates/writingdeck-sys/src/linux.rs` unit tests; `src/shell_ui_tests.rs`

**Interfaces:**
- Produces: `parse_time_sync` maps `NTP=yes|no` and `NTPSynchronized=yes|no` to `Some(true|false)`; any other value or a missing line gives `None` for that field.
- Row text, exactly: `Network time: on · synchronized`, `Network time: on · not yet synchronized`, `Network time: off`, or `Network time: unknown`, then the existing `Timezone: …` row and `Choose timezone`.

- [ ] **Step 1: Write the failing tests**:
  - `parse_time_sync_reads_yes_no_and_ignores_anything_else` (linux.rs): `"NTP=yes\nNTPSynchronized=no\n"` gives `(Some(true), Some(false))`; `"NTP=maybe\n"` gives `(None, None)`; `""` gives `(None, None)`.
  - `date_time_panel_says_whether_the_clock_is_set_from_the_network` (shell_ui_tests.rs): with the simulated snapshot, Date and time shows `Network time: on · not yet synchronized`; with both `None` it shows `Network time: unknown`; the old "not available yet" text is gone.
- [ ] **Step 2: Run them to verify they fail** — `cargo test -p writingdeck-sys parse_time_sync` and `cargo test -p grimoire-tui date_time_panel`. Expected: FAIL.
- [ ] **Step 3: Implement** the fields, the invocation (a failed run leaves both `None`), the parser and the rows. In `docs/API_AND_NETWORK_USE.md` list the new automatic contacts: `systemd-timesyncd` (Debian's NTP pool) and `apt-daily` (Debian archive index refresh); say no packages install automatically. In `docs/NATIVE_TIMEZONE.md` describe the row.
- [ ] **Step 4: Run** `cargo fmt --all -- --check`, `cargo clippy -p writingdeck-sys -p grimoire-tui --all-targets -- -D warnings`, and both test commands — expected: clean and OK.
- [ ] **Step 5: Commit** — `git commit -m "Show network time state in Settings > Date and time"`.

### Task 4: Rebuild and qualify in the VM

**Files:**
- Modify: `os/tests/rc_vm_phases.py` (`phase_installed_boot`: new checks)
- Modify: `docs/RC_CONTRACT.md` (decision 5 note: the target now carries Debian's online sources; WrOS bundles remain the WrOS update path), and the update section of `docs/RC_RESCUE.md` (`sudo apt update && sudo apt upgrade` for Debian security fixes until online WrOS updates exist)
- Test: `os/tests/test_rc_vm.py` (fake-guest test for the new checks)

**Interfaces:**
- Consumes: Tasks 1-3.

- [ ] **Step 1: Write the failing harness test** `test_installed_boot_checks_debian_sources_and_network_time` in `os/tests/test_rc_vm.py` using the existing fake guest: the phase records `ctx.check('Debian online sources installed', …)` true only when `/etc/apt/sources.list.d/debian.sources` equals `/usr/share/wros/apt/debian.sources`, and `ctx.check('network time service enabled', …)` true when `systemctl is-enabled systemd-timesyncd` prints `enabled`.
- [ ] **Step 2: Run it to verify it fails** — `python3 -m unittest test_rc_vm -k debian_sources`. Expected: FAIL.
- [ ] **Step 3: Implement** the two checks in `phase_installed_boot`, plus a recorded (not asserted) `timedatectl show` result; update the two docs.
- [ ] **Step 4: Rebuild and run** — `rc-iso-build.sh <suffix>` from `main`/`development` HEAD; then VM phases A (`install --layout colemak_dh`), B (`installed-boot`) and C (`update --variant ok`, with a fresh signed bundle). Expected: all PASS with the machines networkless (`-nic none`), proving the offline install, the new checks, and an unchanged WrOS update.
- [ ] **Step 5: Commit** — `git commit -m "Qualify Debian sources and network time in the RC VM run"`.

## Not in this plan

- Applying Debian updates through WrOS's review and offline apply (needs the online update channel design).
- Moving existing rc1 installs: they have no template, so write the Global Constraints content to `/etc/apt/sources.list.d/debian.sources` by hand, then `sudo apt update && sudo apt install systemd-timesyncd`.
- A Settings toggle for network time (would need a privileged `timedatectl set-ntp` path).
