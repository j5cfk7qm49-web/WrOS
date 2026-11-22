# rc23: smaller updates and no dashboard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship rc23:
- updates of about 20 MB instead of 72 MB (no repository copy, a stable initrd, thin online bundles from cache.nixos.org);
- a desk with no WrOS dashboard (small productivity dialogs, a calendar month grid, rest day and challenge in the bottom bar, an install prompt on the live stick).

**Architecture:**
- **System and updater:** the update half is the Nix modules (`nixos/modules`, `nixos/pkgs/rust-packages.nix`), the bundle builder (`tools/build-nixos-update-bundle.py`) and the update engine (`nixos/scripts/wros-update-engine.py`). Upstream files are listed in the signed manifest with cache.nixos.org addresses and the same SHA-256 checks.
- **Desk:** the dashboard half is Grimoire's WrOS deck (`src/app/wros.rs`), productivity overlay (`src/app/productivity.rs`) and menus.
- **Who does what:** the two halves touch separate files. The desk half goes to Codex in `WrOS/rc23-codex`, one task at a time; the update half is mine in `WrOS/rc23`.

**Tech Stack:** Nix/NixOS modules and VM tests, Python (update engine and builder), Rust/ratatui.

**Spec:** `docs/superpowers/specs/2026-11-21-rc23-thin-updates-no-dashboard-design.md`

## Global Constraints

- Every file the engine imports is checked against the SHA-256 and size in the signed `closure-manifest.json` before import. No new trust root.
- Upstream addresses: exactly `https://cache.nixos.org/` + `[0-9a-z]{32}.narinfo` or `nar/[0-9a-z]{52}.nar(.xz|.zst|.bz2)?`. HTTPS only, no redirects. A test override exists only through the engine's constructor and arguments, like `ONLINE_URL`.
- The WROS-UPDATE stick keeps a full bundle. The release publishes both: the full `wros-nixos-updates.tar` and the thin `wros-nixos-updates-thin.tar`, which Pages serves.
- No mirrors or second host.
- rc22's `workspaces.json` dashboard fields are kept and never rewritten away.
- "Project", never "book", in new text. Keys: don't take a key that `assets/help/24-every-key.md` lists. F12 is freed.
- Run `rc23-work/fmt.sh` after Rust edits and the **python-suite before qualifying** (gates don't run it).

## Review Focus

1. **A thin update fetched while cache.nixos.org is unreachable or slow:** the fetch fails with a clear message ("couldn't reach cache.nixos.org"), nothing is staged, and a retry resumes. → Task 3 test `fetch_reports_an_unreachable_upstream_and_stages_nothing`.
2. **cache.nixos.org has garbage-collected an old path** (404 on a narinfo listed months ago): the error names the missing package, and offline media still works. → Task 3 test `a_missing_upstream_file_is_named_and_stages_nothing`.
3. **A stick with a thin bundle by mistake:** it is refused offline with "this bundle needs the internet; use the full one from the release". → Task 3 test `an_offline_thin_bundle_is_refused_with_a_reason`.
4. **A writer used to F12 or Esc landing on the dashboard:** F12 does nothing, and Esc on WrOS pages returns to writing. The tutorial no longer tells them to press F12. → Task 5 test `f12_and_esc_never_open_a_dashboard`.
5. **Calendar navigation outside the schedule, or across year ends:** days outside the schedule are dimmed and can't become rest days, and `]` from December goes to January of the next year. → Task 7 test `calendar_grid_dims_outside_the_schedule_and_crosses_years`.

---

### Task 1: No repository copy in the system

**Files:**
- Modify: `nixos/modules/guards.nix:50-62`, `nixos/modules/polkit.nix:174-181`, `nixos/modules/live.nix:191`, `nixos/modules/boot.nix:331`
- Modify: `flake.nix` (a new check, `no-source-in-closure`)

- [ ] **Step 1: Write the check.** `checks.no-source-in-closure` is a `runCommand` that reads `closureInfo { rootPaths = [ toplevel ]; }` and fails if any line equals `${self}` (the flake's source store path). Run `rc14-argon/nixos-wsl/build.sh rc23s checks.x86_64-linux.no-source-in-closure`. Expected: FAIL.
- [ ] **Step 2: Rewrite each `"${wrosRoot}/x"` as `wrosRoot + "/x"`.**
  - Inside `readFile`, `nixPam` and `iniValues`, pass the path itself.
  - In `boot.nix:331`, use `${wrosRoot + "/nixos/scripts/wros-boot-logo"}`.
  - `git grep -n '\${wrosRoot}' nixos flake.nix` must print nothing.
- [ ] **Step 3: Run the check.** Expected: PASS. Then run `checks.x86_64-linux.boot-unlock-login` (pam.d/vlock and wros-recovery, logind and sleep settings) and `recovery-menu`. Expected: PASS.
- [ ] **Step 4: Commit** `rc23: the system no longer carries a copy of the WrOS repository (44 MB)`.

### Task 2: A boot image that stays the same between releases

**Files:**
- Modify: `nixos/pkgs/rust-packages.nix` (`wros-boot`)
- Modify: `flake.nix` (check `initrd-stable`)

- [ ] **Step 1: Write the check.** `checks.initrd-stable` evaluates the installed system twice, with `version = "0.1.0~rc23"` and `version = "0.1.0~rc99"`, and asserts that `config.system.build.initialRamdisk.drvPath` is equal (an eval-only `runCommand` that compares two strings). Expected: FAIL.
- [ ] **Step 2: Stop `wros-boot` changing when the release does.**
  - Build it with a fixed `version` (its crate's `Cargo.toml` version, read with `lib.importTOML`).
  - Give it a source fileset of only `Cargo.lock`, the workspace `Cargo.toml`, `crates/wros-boot` and the crates it depends on (from its `Cargo.toml` path dependencies), plus a stub `src/main.rs` if Cargo needs the root package's target to exist.
  - Check that `wros-boot`'s output still embeds nothing version-specific: `grep -c 0.1.0~rc` on the binary is 0.
- [ ] **Step 3: Run `initrd-stable`.** Expected: PASS. Then run `boot-unlock-login`, `boot-seams` and `start-frames`. Expected: PASS.
- [ ] **Step 4: Commit** `rc23: the boot image stays the same unless boot code changes (35 MB per update)`.

### Task 3: Thin update bundles

**Files:**
- Modify: `tools/build-nixos-update-bundle.py` (a `--thin` flag)
- Modify: `nixos/scripts/wros-update-engine.py` (`parse_closure_manifest` URL check, `fetch_online`, an offline refusal for thin bundles)
- Modify: `rc14-argon/nixos-wsl/release-build.sh` (outside the repo; build both bundles)
- Modify: `.github/workflows/release-updates.yml` (deploy the thin tar)
- Test: `os/tests/test_nixos_bundle_tool.py`, `os/tests/test_update_live.py` (or the engine's existing tests), a new VM test `nixos/tests/thin-update.nix`
- Modify: `docs/API_AND_NETWORK_USE.md`

**Interfaces:**
- Produces:
  - `UPSTREAM_URL = 'https://cache.nixos.org/'` in the engine;
  - `UPSTREAM_URL_RE` (production pattern);
  - `fetch_online(..., upstream_url=UPSTREAM_URL)` (test override);
  - the manifest field `"thin": true`;
  - builder flag `--thin --upstream-url URL` (default cache.nixos.org).

- [ ] **Step 1: Failing tests (Python, fakes over a local HTTP server):**
  - `thin_bundle_lists_upstream_files_with_their_cache_address`: the builder against a fake cache that has path A (same NarHash) but not B gives a manifest where A's two files carry `url`s under the fake cache and are absent from the bundle, B's files are in the bundle, and `thin` is true.
  - `a_differing_nar_hash_stays_in_the_bundle`.
  - `fetch_downloads_upstream_files_and_imports_like_hosted_ones`.
  - `a_tampered_upstream_file_is_refused` (wrong SHA-256, so the fetch fails and `partial` is gone).
  - `fetch_reports_an_unreachable_upstream_and_stages_nothing`.
  - `a_missing_upstream_file_is_named_and_stages_nothing`.
  - `an_offline_thin_bundle_is_refused_with_a_reason` (stage from a directory with `thin: true` while upstream files are missing → "This update needs the internet: use the full bundle from the release on a stick.").
  - `upstream_urls_must_be_cache_nixos_org` (the parser refuses `https://evil.example/nar/…`, `http://cache.nixos.org/…`, a path traversal, and an upper-case hash).
- [ ] **Step 2: Run the python-suite** (`build.sh rc23p checks.x86_64-linux.python-suite`). Expected: those tests fail.
- [ ] **Step 3: Implement.**
  - **Builder:** for each kept narinfo's store path, `GET {upstream}/{hash}.narinfo`. If it is 200 and `NarHash` equals the narinfo in the closure dir, `GET` its `URL:` archive, check that the archive decompresses to that NAR hash (`nix-store --dump`-free: `xz`/`zstd` decompress, then `nix hash path --type sha256` over the NAR via `nix-store --restore` into a tmp dir, or `nix hash file` of the NAR), record both files' sha256, size and url, and drop them from staging.
  - **Engine:** `fetch_online` already uses `item.get('url')`. Widen the URL check, pass `upstream_url` through for tests, and make `stage_update` from a directory refuse a `thin` manifest whose upstream files aren't present.
- [ ] **Step 4: Run the python-suite.** Expected: PASS.
- [ ] **Step 5: VM test `thin-update`.**
  - Build a thin delta from rc23 to a variant toplevel, as `update-goback` builds its update.
  - Serve a mini cache over HTTP in the VM (python `http.server`) for the upstream files and the channel for the rest.
  - Fetch and stage online from it, restart into the new generation, and check `/etc/wros-version`.
  - Corrupt one upstream file and check the fetch is refused and nothing is staged.
  - Expected: PASS.
- [ ] **Step 6: Release pieces.**
  - `release-build.sh` makes the full and thin bundles (thin with `--thin`).
  - The workflow deploys the thin tar's tree to Pages and keeps the full tar as a release asset.
  - `docs/API_AND_NETWORK_USE.md` lists cache.nixos.org: when, and what is sent (package file names only).
- [ ] **Step 7: Commit** `rc23: thin online updates fetch upstream packages from cache.nixos.org, checked against our signature (#169)`.

### Tasks 4–8: The desk without a dashboard (Codex, sequential, in `WrOS/rc23-codex`)

Each task is one Codex brief, `rc23-work/issue-<name>.md`. I review, build and test each one before the next starts, then squash-merge into `rc23`.

### Task 4 (Codex): Productivity dialogs

**Files:**
- Modify: `src/app/productivity.rs` (drawing, input), `src/app/overlays/bar.rs` (Tools › Progress submenu), `src/palette.rs`
- Modify: `assets/help/33-productivity.md`
- Test: the screen tests in `src/ui_tests.rs` or a new `src/productivity_ui_tests.rs`

- [ ] Each page (Overview → **Edits**, Goals, Calendar, Sprints, Achievements → **Collection**, Challenges) opens as a centred box at most 76×22, without the "1 edits 2 goals…" row.
- [ ] Tools gains **Progress ›** with Goals…, Calendar…, Sprints…, Edits…, Collection… and Challenges…, which run the existing actions. The palette keeps one entry each.
- [ ] Tests:
  - `each_progress_dialog_opens_alone_from_tools_progress`;
  - `no_progress_dialog_draws_the_tab_row`;
  - `progress_dialogs_fit_80x24_and_the_hp_stream_grid`.

### Task 5 (Codex): The dashboard goes

**Files:**
- Modify: `src/app/wros.rs` (Home and Widgets pages, `open_dashboard`, `launch_dashboard` use, Esc returns), `src/app.rs` (F12 → nothing), `src/app/overlays/bar.rs` (remove WrOS › Overview), `src/palette.rs`, shortcuts (`Shortcut::Dashboard`)
- Modify: `crates/grimoire-core/assets/wros/tutorial.json` (the F12 step, plus its sha256 in `docs/content-licenses.json`), help `24-every-key.md`, `27-the-menu.md`, `28-wros.md`, `23-settings.md`
- Test: as below

- [ ] Remove the Home and Widgets pages and their drawing. `launch_dashboard` is read but ignored, and never written false.
- [ ] Esc on any WrOS page closes to the writing (or back to the page that opened it).
- [ ] The old `Shortcut::Dashboard` entry in a saved shortcuts file is ignored, not an error.
- [ ] Tests:
  - `f12_and_esc_never_open_a_dashboard`;
  - `a_saved_dashboard_layout_still_loads_and_is_kept`;
  - `an_old_shortcuts_file_with_dashboard_still_loads`;
  - menu and every-path tests updated.

### Task 6 (Codex): The live stick's install prompt

**Files:**
- Modify: `src/app/wros.rs` (start-up), the live-session tests

- [ ] On live media, start-up opens a small dialog: **Install WrOS to disk** (runs the existing install action) / **Try it first** (closes). It shows once per boot (a flag in `/run/user/$UID`, or app state for the session).
- [ ] **WrOS › Install WrOS to disk…** stays.
- [ ] Tests:
  - `live_media_asks_once_install_or_try`;
  - the VM tests `live-session` and `live-graphical` updated to the new start screen.

### Task 7 (Codex): Calendar month grid

**Files:**
- Modify: `src/app/productivity.rs` (Calendar dialog), `crates/grimoire-core/src/productivity.rs` (helpers only if needed: `month_days(year, month) -> Vec<NaiveDate>`, `in_schedule(date) -> bool`)
- Test: as below

- [ ] A 7-column month grid starting on Monday. Arrows move by a day or a week, Enter toggles a rest day (`toggle_rest`), `[` and `]` change the month, and Home goes to today.
- [ ] Days outside the schedule are dimmed and refuse Enter with a message. The schedule's settings rows stay below the grid.
- [ ] Tests:
  - `calendar_grid_marks_rest_days_and_today`;
  - `calendar_grid_dims_outside_the_schedule_and_crosses_years`;
  - `enter_toggles_a_rest_day_and_saves`.

### Task 8 (Codex): The bottom bar's rest day and today's challenge

**Files:**
- Modify: `src/ui.rs` (`draw_status`), `src/app.rs` (a once-a-day message)

- [ ] When today is a rest day, the bottom bar shows `rest day` instead of today's goal figure.
- [ ] The first desk open each day shows today's challenge once as `msg`, remembered by date in app state.
- [ ] Tests:
  - `the_bottom_bar_says_rest_day_on_a_rest_day`;
  - `todays_challenge_is_said_once_a_day`.

### Task 9: Release

- [ ] Version `0.1.0~rc23` (`flake.nix`, `os/VERSION`, `os/tests/test_packaging.py`).
- [ ] Release scripts `rc14-argon/nixos-wsl/rc23-*.sh` (baseline rc22; `channel-src/rc22` = `git archive 9191ab0`). Add the new checks `no-source-in-closure`, `initrd-stable` and `thin-update` to the qualification list.
- [ ] Run the python-suite, then the full qualification. Check the Rufus pictures.
- [ ] Release notes: download size, both bundles, cache.nixos.org, the dashboard gone, F12 freed, Tools › Progress, the calendar grid.
- [ ] Build, sign, publish and run the channel check as an rc22 laptop. Measure the thin download against the full one.
