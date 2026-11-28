# WrOS rc34 Plymouth and Wayland Implementation Plan

> Draft for the owner's approval. Work through the checklists in order. Each task
> identifies tests to make fail before implementing it.

**Goal:** Boot installed WrOS through Plymouth and open Grimoire in Cage with the same
Unifont picture. Keep the kernel-console TUI, existing lock and recovery paths.

**Architecture:** The systemd initrd uses Plymouth's password agent and immutable
WrOS theme assets. The existing GRUB appender publishes rc33 firmware followed by a
bounded theme archive. The native greeter hands off to the writer's own Cage process.
A software Wayland client presents the existing raster. An unprivileged session
process owns scheduled gamma, and wluma is a separate hardware trial.

**Tech stack:** NixOS, systemd, GRUB, Plymouth, packaged Cage and wlroots, Rust
Ratatui/Unifont, Python stdlib for machine settings and archive tooling, QEMU.

**Spec:** [rc34 design](../specs/2026-11-28-rc34-plymouth-wayland-design.md).
Question numbers below refer to its *Questions for the owner*.

## Global Constraints

- Keep the native DRM default until the release switch in Task 7. The permanent
  text-start and login console paths outrank session settings and diagnostics.
- Leave the lock, suspend, restart, shutdown and installer policies and privileged
  helpers intact. Limit privileged changes to the installed boot path and its fixed
  settings helper. Preserve the rc33 firmware CLI, sets, seed, and update contract.
- Theme preferences and receipts are machine-local. No laptop rebuild, network request,
  caller-controlled archive member, credential copy or general-purpose archive extractor.
- Keep changes in WrOS-owned files. Shared files receive small hooks only. Reuse the
  pinned Wayland and XKB packages already in the lockfile. Ask before any dependency
  addition the pinned set cannot support. Use synthetic fixtures and no real network.
- In this implementation plan, `check.sh <mode>` and `vm.sh <name>` mean, from Git Bash,
  `MSYS_NO_PATHCONV=1 wsl -d NixOS -- bash /mnt/c/Users/lettuce/Documents/ChatGPT/WrOS/rc33-work/<script> <argument>`.
  Run serially with this checkout's pin. Commands below are for implementation, after
  the owner approves the draft.

## Review Focus

1. Test both archived parts independently and in the kernel. A broken theme must
   leave usable firmware. Failed firmware cannot silently publish a theme-only initrd.
2. Test who reads the password and owns tty1 at every stage, including all four layouts,
   retries and text mode. Keep Plymouth's packaged shutdown hooks out of this change.
3. Require actual frames and seat ownership in VM assertions. A process, frame callback
   or ready file alone cannot prove scanout, unlocking or input isolation.
4. Preserve signed identity, old GRUB entries, Go back, probation, and the live installer
   path. Review every privileged behavior change line by line before changing the default.
5. Collect Stream measurements as acceptance evidence. Keep functional and safety
   regressions separate from performance comparisons.

### Task 1: Share the per-generation GRUB appender

**Files:** `nixos/modules/installed.nix`, `nixos/pkgs/helpers.nix`,
new `nixos/scripts/wros-initrd-append`, `os/scripts/wros-boot-settings`;
extend `os/tests/test_wros_firmware.py`, `os/tests/test_boot_settings.py`,
`nixos/tests/firmware-set.nix`, `thin-update.nix` and `update-goback.nix`.
Use a narrow locally packaged GRUB integration only if needed to cover publication and
verified reuse. **Depends on Q6.** Default is all retained compatible entries.

**Interfaces:** `append-initrd-secrets <GRUB temp path>` resolves the calling toplevel.
`wros-initrd-append build <toplevel> <GRUB temp path>` consumes the rc33 firmware CLI and
one Settings snapshot. `refresh-themes` is internal to the authorized queue worker and
resolves only currently listed, compatible boot entries. Its theme archive contains
`wros-plymouth/palette.v1` (canonical versioned RGB data, at most 512 bytes) and its
own directory. Store root-only per-generation firmware prefix receipts (length, SHA-256)
on encrypted storage. Keep the previous receipt until successful publication is
observable. Keep the old ESP copies for rc33 and for stage-1 layout and text selection.

- [ ] Add failing Python tests `test_appender_packs_two_complete_disjoint_archives`,
  `test_invalid_theme_keeps_firmware`, `test_failed_firmware_reuses_only_a_verified_prefix`
  and `test_refresh_replaces_only_the_palette_tail`. Assert exact member types/paths,
  byte-identical firmware, bad-input and symlink rejection, default palette with one
  producer down, previous bytes on interrupted write or low space, and another run for
  a save during refresh. Run `check.sh py`. Expected red for the new interfaces.
- [ ] Add failing VM coverage in `firmware-set` for GRUB's actual load order and a
  stage-1 firmware request and palette read. Add a selected and retained generation,
  rc33 rollback, mixed-version appender calls, refresh/update serialization and an old
  entry eligible for checked reuse after failed publication. Run `vm.sh firmware-set`.
  Expected red at supplemental palette or rewrite assertion.
- [ ] Implement the producers and publication. Hold the GRUB writer lock through the
  installer's final rename and cleanup. Join the update transaction lock for refresh,
  with queue locks dropped. Audit the current and prior-generation bootloader paths
  and ensure the original accepted archive was verified before authorizing reuse.
  Keep the 64 MiB ESP reserve including temporary files. Publish only a complete
  candidate at the path GRUB examines, including when old GRUB ignores the appender's
  failure status for a retained entry. A fresh generation without verified firmware
  retains rc33's bootloader error semantics.
- [ ] Run `check.sh py`, `vm.sh firmware-set`, `vm.sh thin-update`,
  `vm.sh update-goback`. Expected green with unchanged main initrd and toplevel after
  theme refresh, and byte-identical saved firmware after Go back.

### Task 2: Use Plymouth in the installed systemd initrd

**Files:** `nixos/modules/boot.nix`, new WrOS Plymouth theme files under
`nixos/plymouth/`, a new Plymouth module imported by `installed.nix` if needed;
`nixos/tests/boot-unlock-login.nix`, `unlock-waits.nix`, `start-frames.nix` and
`os/tests/test_boot_unlock.py`. **Depends on Q5.** Default is systemd-cryptsetup
with Plymouth and the independent text fallback.

**Interfaces:** Stage-1 setup reads `/boot/wros-boot/{keyboard.json,boot-screen}`
with existing read-only mount and bounds. It labels the effective console and Plymouth
layout from the same allowlist. It reads the versioned palette at its fixed initrd path
and materializes a runtime script from the shipped template using numeric substitutions.
With no key file in crypttab, cryptsetup talks to the selected systemd password agent.
`wros.unlock=text`, saved text, and graphical failure all leave a usable console prompt.

- [ ] Add failing tests `test_plymouth_plan_accepts_only_fixed_layouts_and_palette`
  and `test_text_choice_bypasses_plymouth_even_with_a_palette`. In the encrypted
  VM enter distinct layout-sensitive characters with QWERTY, Colemak, DH ANSI and
  DH ISO. Run `check.sh py` and `vm.sh boot-unlock-login`. Expected red on the
  new palette or agent/layout assertions.
- [ ] Keep the existing root wait and LUKS options. Wire the shipped Plymouth assets,
  start/ask ordering and switch-root carryover. Use a bounded ordinary quit before
  greetd until Task 3 qualifies splash retention. Disable both native splash units
  when Plymouth is selected so the two renderers never compete in this step. Confirm
  the pinned Plymouth units, direct-input/XKB path and password agent timing in the
  built image before enabling it. Keep the console map for text prompt takeover
  and remove the native ramfs password path from this installed configuration.
  Use plain text if layout selection, display setup, the agent, or Esc takeover
  fails. Disable Plymouth's shutdown and reboot units in both stages and check
  their package-provided dependencies.
- [ ] Run `check.sh py`, `vm.sh boot-unlock-login`, `vm.sh unlock-waits` and
  `vm.sh start-frames`. Expected green for wrong then correct password, the long
  wait, disabled graphics, invalid/missing palette and all text paths. Frame captures
  and journals must show the prompt is visible and credentials are absent from logs
  and passphrase files.

### Task 3: Hand Plymouth over to the native greeter

**Files:** `nixos/modules/boot.nix`, `session.nix`, `nixos/pkgs/greetd-handover.patch`,
`nixos/plymouth/`, `crates/wros-greeter/src/main.rs`, WrOS-owned welcome view;
extend `nixos/tests/boot-seams.nix`, `start-frames.nix` and `boot-unlock-login.nix`.
**Depends on Q1 and Q5.** Default is the current greetd/PAM greeter, native or text.

**Interfaces:** One bounded release point immediately before the login tty1 opens.
The main theme draws the fixed progress milestones and WrOS mark. Map boot milestones
to actual initrd and stage-2 readiness. Continue the existing palette and mark into
the greeter and existing first-page or welcome surface as space permits.

- [ ] Extend the seam VM to assert real milestone and login frames in order,
  quit/quit-wait bounded even with a missing daemon, text fallback visible when a
  handoff fails, and safe VT setup on repeat login. Assert Plymouth stays off the
  shutdown/restart screen. Run `vm.sh boot-seams`. Expected red at Plymouth's
  release or frame milestones.
- [ ] Qualify splash retention at the Plymouth release point and retire rc33's
  native splash token. Preserve the boot-ok service and GRUB pending semantics. Keep greetd's
  normal VT setup except where verified splash retention requires a narrower change.
  Keep the existing authentication, greeter power actions and keyboard controls.
- [ ] Run `vm.sh boot-seams`, `vm.sh start-frames`, `vm.sh boot-unlock-login`.
  Expected green for normal, failed and text starts.

### Task 4: Add the single-client Wayland session as an opt-in

**Files:** `crates/wros-graphics/src/lib.rs`, WrOS-owned Wayland presenter/input
module (proposed `src/wayland.rs`), `src/display.rs`, `src/display/backend.rs`,
`src/session.rs`, `os/scripts/wros-session`, `nixos/pkgs/{rust-packages,paths,helpers}.nix`,
minimal feature/wiring in shared `Cargo.toml` and `src/main.rs`; extend
`os/tests/test_session_launcher.py`, `tests/driven.rs`, `nixos/tests/keyboard-preview.nix`,
and add `nixos/tests/wayland-session.nix` in `flake.nix`. **Depends on Q3 and Q4.**
Default is `wl_shm`, with old trials preserved and the live launcher unchanged.

**Interfaces:** `WROS_SESSION=wayland`, explicit `backend=wayland` during development,
existing `WROS_READY_FILE` and editor event/display contracts. Use the same raster
for the same cells. Honor xdg configure, output/buffer scale, compositor cursor,
frame callbacks and buffer release. Derive cell hit testing from scale. Use the
compositor keymap for logical keys, repeat, compose and focus reset. Reuse locked
Wayland/XKB dependencies. Keep the editor a direct descendant of Cage with tty1
available to the existing lock call.

- [ ] Add failing tests `wayland_shm_matches_unifont_pixels_and_cell_hits`,
  `wayland_buffers_wait_for_release`, `wayland_drops_input_on_focus_loss`,
  and launcher cases for explicit selection, a startup failure, two failures and
  no fallback after ready. VM and driven cases must prove actual frames, Colemak,
  compose, scrolling, cursor motion and rendering after resize. Exercise the keyboard
  preview timeout and rollback. Add the Wayland feature to the packaged driven suite.
  Run `check.sh all`, `check.sh py`, `vm.sh rust-driven-suite` and
  `vm.sh wayland-session`. Expected red at the new client and launcher assertions.
- [ ] Implement using a bounded buffer pool and redraw coalescing. A frame callback
  arms existing readiness and probation signals only after the first real drawing
  commit. Bound start and confirm seat release before console fallback. Determine
  whether live keyboard preview can use checked pending and committed layout values
  via a narrow compositor update. If that needs a broader protocol change, keep the
  timed preview reachable in the console and explicitly label when Wayland takes the
  confirmed choice on next login. Test the selected contract. Gate any IME claim on a
  compositor protocol test.
- [ ] Run `check.sh all`, `check.sh py`, `vm.sh rust-driven-suite`,
  `vm.sh wayland-session`, `vm.sh keyboard-preview` and `vm.sh live-graphical`.
  Expected green with no change to the live install dashboard path.

### Task 5: Qualify the existing VT lock under Cage

**Files:** WrOS Wayland display/input adapter and display hooks from Task 4,
new `nixos/tests/wayland-lock.nix` in `flake.nix`, reused
`os/tests/console_lock_vm.py` and `nixos/tests/console-lock*.nix` fixtures.
**Depends on Q2.** Default is existing tty8/PAM handoff and no lock-helper changes.

**Interfaces:** Feed the existing display suspend, `service_locked` and resume
contract. Retire the client event queue and visible frame before invoking the existing
helper. Logind/libseat transfer DRM/input access on the VT switch. Never dispatch client
input or repaint writing until the existing authenticated result permits it.

- [ ] Add failing VM scenarios for normal lock, wrong password, attempted VT switch,
  killed locker, compositor death, no proof, post-suspend unlock, stale input and a
  second output. Assert active tty8 and no writing frame/input before authentication.
  Run `vm.sh wayland-lock`. Expected red until the presenter observes the existing
  lock contract under real VT ownership.
- [ ] Implement presenter quiesce/resume. Keep the existing helper identity, tty8
  ownership, PAM evidence, guards, return codes and fail-closed behavior. If safe VT
  transfer cannot be demonstrated without changing policy, keep Wayland opt-in and
  bring the result back to the owner.
- [ ] Run `check.sh all`, `vm.sh wayland-lock`, `vm.sh console-lock`,
  `vm.sh console-lock-text` and `vm.sh console-lock-vlock`. Expected green, with
  physical DRM and sleep behavior separately checked on the Stream.

### Task 6: Trial and integrate session display controls

**Files:** `nixos/pkgs/{default,paths,helpers}.nix`, `os/scripts/wros-session`,
new unprivileged night-light adapter and timezone data, `crates/wros-sys/src/display.rs`,
`src/app/shell/display.rs` and the existing display page and help.
Extend `os/tests/test_display_manager.py`, `test_session_launcher.py` and
`src/shell_ui_tests.rs`. **Depends on Q7.** Default is sunsetr after the brief
Stream trial, neutral fresh-install day and opt-in experimental wluma.

**Interfaces:** One gamma owner in the new Wayland session. Bounded Display settings
store on/off and day/night temperatures and select the offline solar schedule from
`/etc/localtime` with a documented fixed-hours fallback. Preview reverts on deadline
or failed IPC even if the editor dies. Apply after resume/output arrival. Manual
brightness is brightnessctl, with exclusive handover before a trial wluma service can
write that backlight. A webcam has an independent default-off consent setting.

- [ ] Add failing tests `test_equal_temperatures_stay_warm_all_day`,
  `test_a_saved_warmth_setting_migrates_to_constant_warmth`,
  `test_timezone_schedule_handles_dst_and_missing_zone`,
  `test_preview_reverts_without_the_editor`, and
  `test_resume_and_hotplug_reapply_without_a_second_gamma_owner`.
  Add fake service tests that fail if wluma opens a capture or webcam by default or
  contests manual brightness. Run `check.sh all` and `check.sh py`. Expected red
  at the new settings, scheduling and launch interfaces.
- [ ] Trial pinned sunsetr under Cage on the Stream. Record exact backend, config
  and IPC semantics, supported gamma controls, fades, constant warmth and restart
  behavior. Wire the verified interfaces, retaining the preview/Keep/rollback flow.
  Trial wluma with `capturer = "none"`, hardware and no-sensor cases, and a separate
  permission check for an opt-in webcam. If the trial is unsuitable, record the reason
  and keep manual brightness. Keep all dependencies local.
- [ ] Run `check.sh all`, `check.sh py`, `vm.sh wayland-session` and
  `vm.sh wayland-lock`. Expected green for single gamma ownership, unsupported
  hardware, restart and fixed fallback. Record measured colour and backlight behavior
  on actual hardware.

### Task 7: Switch the installed default and finish the release checks

**Files:** Session files and tests from Task 4, `assets/help/{23-settings,28-wros}.md`
and any affected menu, key or context topic, `docs/{GRAPHICAL_SESSION,WARM_SCREEN,HARDWARE_CHECKLIST,RELEASE_CHECKLIST}.md`,
`CHANGELOG.md`, `README.md`. **Depends on Q1–Q7.** Defaults are the recommendations
above. The live ISO remains on its native/console path, with permanent text fallback.

- [ ] Add failing default-selection tests for fresh installs and upgraded graphical
  users, with absent and saved stock `backend=drm` preferences. Preserve explicit
  console and trial choices. Provide a distinct diagnostic native selection since rc33
  wrote `drm` for ordinary graphical use. Preserve fallback and update probation.
  Run `check.sh py` and `vm.sh wayland-session`. Expected red at the installed
  default until the flip.
- [ ] Once Tasks 1–6 pass their applicable functional checks, switch installed
  graphical sessions to Wayland, preserve manual diagnostics and document the
  native-live exception. Update help for visible boot, login, warmth and recovery
  behavior in the same change. Search for and correct the old instructions.
- [ ] Run `check.sh all`, `check.sh py`, `check.sh gates` and
  `python tools/check-wros.py`. Run the Task 1–6 VMs serially on the release candidate,
  plus `vm.sh live-rescue`, `vm.sh install-encrypted` and `vm.sh initrd-stable`.
  Expected green, with real installed and rollback boots, and unchanged installer
  mechanics.
- [ ] Record the Stream results from the design's measurement table. Compare total
  idle/peak memory, physical key-to-panel and scrolling, boot seams, storage, battery,
  hotplug and sleep. Use the Debian figures as historical context. Record safety
  findings and review boot/ESP behavior changes line by line before shipment.

## Outside this plan

Pushing or publishing the release, converting the live installer or greeter to
Wayland, changing the lock implementation, removing the trial renderers before the
rc35 review, and adding theme day variants or GPU page views.
