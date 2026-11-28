# WrOS rc34 Plymouth and Wayland

**Status:** draft for the owner's approval (2026-11-28).

## Why

WrOS maintains roughly 10,000 lines across native graphics, input, greeter and boot.
The fixes in #104, #158, #165, #166 and #176 show the cost of owning display, input and VT
handoffs on a small appliance. Use Plymouth for disk unlock and boot progress, then let
the already packaged Cage own the writing session's display and input. Grimoire remains
one fullscreen client with the same Ratatui layout and Unifont picture. Wayland gives WrOS
a path to compositor cursors, multiple outputs, scaling, screenshots, IME and GPU drawing
for PDF and page views. Actual protocol support depends on the packaged Cage build.

### Goals

- Keep the kernel-console TUI reachable from GRUB and login, and open it automatically
  when Wayland fails to start. Switch the installed default only in the release that
  ships the tested Wayland session.
- Boot and change the next boot's colours offline, using signed system assets and a small
  machine-local colour archive. Keep rc33's per-generation firmware and Go back.
- Carry one WrOS welcome element through boot, login and the first writing page. Show
  what is starting from a short list of real milestones, using WrOS's own artwork (#178).
- Offer offline scheduled warmth and a day temperature for a warm screen all day (#173).
  Trial wluma on the Stream with manual brightness still available (#177).

### Non-goals

- Per-theme day variants, PDF or page rendering, a full desktop, and night light on the
  lock screen. Keep the existing installed lock and live installer contracts.
- A new display manager, new credential protocol, or any change to locking, suspend,
  restart, shutdown or installer behavior. The Plymouth integration is boot-only.

### Anti-goals

- A boot theme that can overwrite executable initrd content or cause firmware to
  disappear. The boot menu and disk prompt must work with missing or malformed settings.
- A splash covering a text password prompt, credentials reaching Grimoire, or a writing
  frame or queued keystroke reaching the session while it is locked.

## How rc33 works today

| Stage | Current behavior and source |
|---|---|
| GRUB | The hidden UEFI menu keeps a WrOS image on screen. Esc opens the menu. A pending flag extends the menu on the next boot if this boot fails to reach greetd. Text start and recovery request the text unlock path. [boot.nix](../../../nixos/modules/boot.nix) |
| Initrd | systemd stage 1 mounts the ESP read-only long enough to read bounded keyboard, theme and screen preferences. It uses compiled console maps for QWERTY and three Colemak layouts. `wros-boot unlock` reads the console, writes a passphrase into root-only ramfs, and cryptsetup uses that key file. Esc or failure gives the console back to systemd's text password agent. The native splash starts after passphrase submission, carries progress through switch-root, and yields to the text agent if needed. [boot.nix](../../../nixos/modules/boot.nix), [unlock.rs](../../../crates/wros-boot/src/unlock.rs) |
| Firmware and theme | The installed toplevel's `append-initrd-secrets` resolves its generation and calls `wros-firmware initrd` into GRUB's temporary output under `/boot`. The uncompressed newc archive actually writes `/wros-firmware/…`, with the kernel instructed where to find it. Stage 2 links that generation's firmware at `/lib/firmware`. The boot settings helper currently queues copies of settings to `/boot/wros-boot`. [installed.nix](../../../nixos/modules/installed.nix), [wros-firmware](../../../nixos/scripts/wros-firmware), [wros-boot-settings](../../../os/scripts/wros-boot-settings) |
| Login | A stage-2 native splash yields tty1 to patched greetd. The unprivileged `_greetd` greeter draws with `wros_graphics::init` (DRM or console), reads console keys, and uses greetd IPC for PAM and the writer-session command. It reads the saved boot palette. [session.nix](../../../nixos/modules/session.nix), [greetd-handover.patch](../../../nixos/pkgs/greetd-handover.patch), [greeter](../../../crates/wros-greeter/src/main.rs), [greetd conversation](../../../crates/wros-greeter/src/lib.rs) |
| Writing | The PAM-authenticated tty1 launcher defaults to native DRM, including on live media. Foot or the OpenGL-first `gui-wgpu` trial can start under Cage on an installed system. The trial tracks first-frame readiness and falls back to the console on startup failure. The private Cage 0.2.0+wros1 package uses wlroots 0.18 and fixes gamma commits. It exposes screencopy. Its source has no ext-session-lock or text-input/input-method manager. [wros-session](../../../os/scripts/wros-session), [window.rs](../../../src/window.rs), [cage.nix](../../../nixos/pkgs/cage.nix), [Cage source](../../../os/vendor/cage/cage.c) |
| Lock and sleep | Grimoire retires input and display. In the window trial it clears the frame and pumps Wayland without dispatching writing events. The privileged helper checks the local tty1/seat0 session, including trusted Cage ancestry for logind's Wayland type, and starts a supervised locker on tty8. The default WrOS lock screen authenticates with PAM and writes proof through its inherited handle. There is a vlock path. Suspend already requires a ready locker. Grimoire resumes only after helper success and input flushing. [console_lock.rs](../../../src/console_lock.rs), [wros-lock](../../../os/scripts/wros-lock), [lock screen](../../../crates/wros-greeter/src/bin/wros-lock-screen/main.rs), [guards.nix](../../../nixos/modules/guards.nix) |

The pinned nixpkgs tree is `0dc498bfe49baeb18b0f9891e800b53bb2603193` from
[flake.lock](../../../flake.lock). Its `nixos/modules/system/boot/plymouth.nix` supplies
systemd initrd assets and switch-root units. It also wires quit at multi-user and
shutdown-related units, so WrOS must explicitly scope the integration to startup.
`system/boot/luksroot.nix` emits a crypttab entry with `-` when no key file is configured.
`system/boot/loader/grub/install-grub.pl` calls the generation's appender with a temporary
path, renames a nonempty result to `/boot/kernels/<system>-secrets`, and adds it after the
main initrd on the same GRUB line. It aborts on a current-entry appender failure and warns
for an older entry. The rc33 spec predates the shipped firmware archive path. Use the
current code as the contract. [rc33 firmware design](2026-11-27-per-machine-firmware-design.md)

## Decisions for rc34

| Decision | Approaches and recommendation |
|---|---|
| Greeter | **Keep the native/console greeter for rc34.** It preserves the greetd/PAM boundary, committed and preview keymaps, and power menu. There is one extra display handoff. A Wayland greeter could run under a separate Cage owned by `_greetd`, and exit before starting the writer's Cage. That removes another native screen but adds a privileged-seat and authentication handoff to qualify. One long-lived Cage across both users would need a new trust boundary for the password form and writer socket. |
| Lock | **Use the existing secured tty8 VT handoff with the WrOS lock screen.** Cage stays in the writer's tty1 logind session. The client submits a cleared frame, gates input, and services Wayland while logind pauses Cage's DRM and input as tty8 becomes active. Resume is governed by the existing helper result. A future ext-session-lock compositor would own blanking/input exclusion but requires a compositor and authentication migration. A Cage overlay is a cosmetic surface and cannot enforce this lock. If the existing VT path cannot be qualified, leave Wayland opt-in until it can. |
| Renderer | **Present the existing `soft_ratatui` Unifont raster via `wl_shm`.** This preserves cell pixels and keeps wgpu, EGL and Vulkan out of the client rendering path. It costs a bounded copy/upload and requires correct buffer release and frame pacing. Alternatively extend `gui-wgpu` with a Unifont atlas or raster texture. That reuses more of the current window but adds GPU startup, font/pixel parity and driver qualification. Keep GPU page views a later decision. |
| Trial and native sessions | **Keep `gui-wgpu`, Foot and native DRM as explicit diagnostic choices in rc34.** Keep the existing manual selections and per-backend failure records. Review removing Foot and wgpu from the shipped image and removing the installed native-writing choice at the rc35 planning review, after rc34 field use. Removing all trials on day one saves closure space but loses direct comparisons and rollback options. Keeping all paths indefinitely preserves the same maintenance load. The kernel-console TUI stays permanently. Keep the live ISO on its existing native/console path while the installer requires tty1 and its native input-reader proof. |
| Unlock | **Keep systemd stage 1 and let systemd-cryptsetup own the prompt through Plymouth's ask-password agent.** Keep the timed ESP read and console `loadkeys` for text unlock. Resolve Plymouth's actual input source and arrange the same committed XKB map before it accepts keys if it reads evdev independently. Prove all four layouts. If layout parity or the agent cannot be qualified, boot the affected configuration with the plain systemd prompt. Keeping `wros-boot unlock` in front of Plymouth preserves current layout behavior but retains the key file, wrong-password seam and extra VT owner. Returning to scripted stage 1 adds a second initrd design and more recovery work. |

## Boot, login, lock and resume

1. **GRUB.** Keep the existing boot entries, failure menu, branding and boot-success
   policy. Load the main initrd first, then the generation's supplemental file. The
   kernel extracts firmware and palette before stage 1. Text start and recovery select
   the systemd console path even if a saved palette exists.
2. **Prepare stage 1.** Preserve the brief read-only ESP lookup and the four known
   console maps. Preserve `boot-screen=text` and accept the legacy `ratatui` value as
   graphical. Prepare Plymouth from a validated palette and checked layout before the
   password agent can read input. Configure the text agent to take over only after
   Plymouth releases input and display, including on Esc, disabled graphics or failure.
   Keep the root device's infinite wait and the existing LUKS device and options.
3. **Unlock and progress.** In the new graphical path, cryptsetup requests and retries
   the password via systemd. Plymouth shows the request and its retry state. Retire the
   native unlock service, ramfs passphrase handoff and native progress services from the
   rc34 installed boot. Continue Plymouth across switch-root. Show fixed messages for
   observed milestones such as *Unlock the disk*, *Opening the disk*, *Starting WrOS*
   and *Opening sign-in*. Avoid raw unit names or device, account and password data.
   A wait for a password carries no progress estimate.
4. **Plymouth to login.** Establish a single bounded release point before greetd can
   take tty1. Pin normal Plymouth quit/quit-wait ordering to that point, use the
   retained-splash option only if it releases devices and avoids a blank frame, and
   follow with a verified text-console recovery on failure. Replace the obsolete native
   splash token use without silently skipping greetd's safe VT setup. Keep the greeter's
   native frame as the intermediate step between Plymouth and the writer's Cage.
5. **Login to writing.** After PAM and greeter teardown, the tty1 launcher runs patched
   Cage as the writer. Cage receives the saved XKB layout. Grimoire gets the configure
   and presents a correctly scaled Unifont frame. Use a compositor frame callback and a
   VM screendump to qualify first-frame readiness. Retain update probation's existing
   started/ready contract and two-failure behavior. Start the console only after Cage has
   exited and released the seat. Never relaunch a second editor after a mid-session crash.
   Use the same welcome mark and palette treatment in Plymouth, the greeter and the first
   page or existing welcome screen where it has room. A returning writer's document
   still opens directly. No extra dialog, delay or source assets from ZacOS.
6. **Lock and resume.** Use the native tty8 locker and existing PAM proof and suspend
   guard. Clearing the client frame alone is insufficient. Prove tty8 has actually
   acquired the display and input before suspend, including when a second monitor is
   attached. Exercise VT switching, failed PAM, killed locker and lost compositor.
   After wake the locker stays active until successful authentication. On return to
   tty1, reset stale keys, buttons, repeat and compose, redraw, then re-apply session
   colour when outputs are usable. Do not weaken the lock helper's caller, proof or
   input checks to accommodate a renderer.

## One GRUB appender and two archives

Keep the same per-generation hook and published filename. The trailing file contains the
rc33 firmware newc archive followed by a separately complete, aligned newc theme archive
with its own trailer. The main initrd owns the Plymouth plugin, fixed WrOS theme
script/template, fonts, logo and default colours. The only mutable theme entries are the
directory `wros-plymouth` and regular file `wros-plymouth/palette.v1`. A v1 record
carries `accent`, `text`, `dim`, `border` and `warning` as six-digit RGB hex, with
the background set by the shipped theme. No caller chooses a member name, mode or
payload path. Bound and revalidate the palette in stage 1 before
materializing a runtime copy of the shipped script with numeric colour substitutions.
Only the shipped theme configuration selects the plugin, script and image paths.

- **The hook** obtains firmware through the existing `wros-firmware initrd <system>
  <file>` contract. Under the same appender, a separate theme producer uses the latest
  committed Settings snapshot. Stage the firmware in a sibling temporary file and
  generate the small palette tail separately. Append it to the staged firmware
  before making GRUB's candidate path visible. A failed tail write must restore
  the verified firmware-only length or discard that candidate. GRUB's older-entry
  error path can publish a nonempty candidate even after an appender failure.
  On a bad or missing theme, emit firmware alone and use the main initrd's defaults.
  Keep the rc33 free-space reserve, including temporary copies and rounding, and prefer
  a firmware-only candidate if the optional tail costs the remaining space.
- **Firmware failure** never leads to a theme-only replacement for a generation that
  requires firmware. An existing verified same-generation firmware prefix can be reused
  for a new theme. Authenticate it against a root-owned receipt on the encrypted root
  keyed by generation, with prefix length and SHA-256. Keep the previous receipt through
  publication until the new prefix is verified in place. Validate the expected format,
  cap sizes and check the digest before copying. If firmware cannot be recovered, keep
  the previous bootable files where possible and follow rc33's GRUB failure policy.
  Keep the theme preference queued independently. No unverified old firmware is copied.
- **Settings refresh** uses the existing authorized `wros-boot-settings` queue. It
  writes a theme archive from the saved palette, then replaces only the verified theme
  tail of each still-listed, compatible generation's supplemental file. Preserve the
  firmware prefix byte for byte. A missing file or receipt leaves that entry untouched
  and reports pending or failed for retry at the next bootloader pass. Mark a refresh
  complete when all eligible entries match its snapshot. If a newer save arrived during
  the run, leave it queued. Keep the legacy ESP copies for keyboard, screen choice and
  rc33 rollback. A Settings refresh neither builds Nix nor changes the GRUB menu, default
  entry, generation selection or pending flag.
- **Publication** uses trusted fixed directories and file descriptors, a same-ESP
  temporary file, sync, and atomic rename. Use a GRUB writer lock covering the entire
  rc34 installer publication and the Settings refresh. Also serialize the refresh
  with the existing update transaction lock for mixed rc33/rc34 Go back and staging.
  The lock order is update transaction, then GRUB writer, and the appender never
  reacquires either. The Settings queue lock is released before acquiring these locks.
  Audit every supported bootloader writer. Prove the old installer can invoke an rc34
  appender and the new one still invokes the rc33 firmware hook. If an older entry's
  published file would otherwise be pruned after a failed append, preserve the original
  only when the checked same-generation receipt can authorize reuse. Verify that behavior
  against the pinned Perl installer before changing its failure handling.
- **Boundaries.** Appender support is per toplevel and format. Themes apply to retained
  compatible generations, so Go back uses the latest machine palette with that
  generation's firmware. Old generations carry their own appender unchanged. Refuse
  symlink, path traversal, special-file and arbitrary-newc inputs. Keep
  `boot.initrd.secrets == {}` as rc33 requires. Receipts and the palette never enter the
  signed toplevel. The physical-offline integrity of the ESP is the existing boot trust
  boundary.

## Night light and automatic brightness

- **Night light (#173).** The pin has MIT sunsetr 0.12.2. First trial it under patched
  Cage on the Stream, checking its actual config and IPC for scheduling, fades, presets,
  daytime warmth and ownership recovery. Use the named zone behind `/etc/localtime`
  and a packaged zone-to-representative-coordinates table. Use solar-elevation bands
  for day, transition and night, and handle DST, extreme latitude and timezone changes.
  Missing or ambiguous zone data gets a fixed local-time schedule with a visible notice.
  No IP geolocation, account or network. Expose *Day temperature* and *Night
  temperature* in Display, with neutral day as the fresh-install default. Identical
  warm temperatures produce constant warmth. Preserve Off and the bounded preview,
  Keep and revert contract. Map an existing enabled constant-warmth setting to equal
  day/night targets, accounting for the existing strength so the upgrade preserves
  its intent.
- Run sunsetr unprivileged inside the authenticated Cage session. Use the existing
  compositor handoff to release native gamma first. Give one process the gamma control
  for each output. The existing compositor warmth worker remains appropriate for the
  retained trials only. Reconcile output changes and resume, using sunsetr's native
  support if verified or a session-local restart/reapply on display reacquisition.
  On failure, preserve manual writing and brightness, show unavailable warmth, and
  release gamma on exit. Keep the native/console warmth and the lock screen's current
  behavior.
- **wluma (#177).** Trial the pinned wluma 4.10.0 with `capturer = "none"` and a
  supported local ambient sensor if present. It is a Wayland/Vulkan application. Verify
  runtime initialization on Gemini Lake even with capture disabled. Treat a missing
  sensor, unusable Vulkan device or insufficient backlight access as unavailable.
  Use only the existing narrow backlight permissions. Brightness remains a physical
  backlight control via brightnessctl for manual changes. During the trial, a manual
  override pauses or stops automation before setting brightness so there are no competing
  writers. Do not start a screen or webcam capture by default. Any webcam sensor needs
  explicit separate consent before opening a device, with a visible off switch. Keep
  wluma off by default and leave it out of the writer UI if the trial cannot give a
  reliable, private improvement.

## Verification

| Where | Required evidence |
|---|---|
| Archive and update VMs | Extend `firmware-set`, `thin-update` and `update-goback`. Inspect the actual GRUB initrd list, request an early firmware fixture in stage 1, assert the palette reached Plymouth and compare firmware bytes across multiple Settings writes. Exercise a failed producer, low ESP space, interrupted and concurrent publication, retained entries, rc33 Go back and the same signed toplevel. |
| Encrypted boot VMs | Extend `boot-unlock-login`, `unlock-waits`, `boot-seams` and `start-frames`. Test all four keymaps with characters that differ, wrong then right passwords, a delayed response, Esc, no graphics, text start, recovery and a bad palette. Assert password prompt ownership, no credential logs/files and one progress-to-login handoff. Verify no Plymouth takeover on shutdown/restart. |
| Session VMs | Add `wayland-session` and `wayland-lock` with a real Cage/logind VT and a DRM-capable QEMU display. Check a painted fullscreen frame and first input, keyboard preview and rollback, scaling and output changes, compositor start/crash fallback, actual tty8 acquisition, no access to writing during lock, failed auth, locker death, resume and stale-key disposal. Run existing `keyboard-preview`, `console-lock`, `console-lock-text`, `console-lock-vlock`, `live-graphical`, `live-rescue` and `install-encrypted` regressions. Use headless Cage for protocol/fixture screenshots in addition to the real-VT cases. |
| Stream 11 | Compare rc33 native and rc34 Wayland on the same panel, project, power mode and kernel where practical. Record cold and warm boot timings and high-frame-rate video of GRUB to disk prompt, password acceptance to progress, Plymouth to login and login to an interactive page. Record seam duration and blank/flash frames. Measure idle and peak total RSS/PSS across the session and helpers, swap pressure, and storage/initrd deltas. Measure real key-to-panel latency with camera or photodiode (median, p95, worst and sample count) and sustained scrolling frame cadence, dropped frames and tearing. Exercise monitor hotplug/scaling, lid suspend, both password layouts in use, gamma at day/night and after wake, brightness override, battery impact and wluma with and without a sensor. Use synthetic projects and no real passwords in recordings. |

The approximately 39 MiB native and 76.5 MiB Cage/Foot figures are Debian-era RSS
reference points. A screenshot timer includes capture overhead. Stream measurements are
rc34 acceptance records and inputs to performance fixes. They do not reopen the
Wayland decision or impose an arbitrary threshold. Recoverable login, password and lock
behavior must still be verified before changing the default. [Graphical trial](../../GRAPHICAL_TRIAL.md),
[historical measurements](../../evidence/rc10-graphical-trial-current.json)

## Risks

- **VT and prompt races.** Plymouth, greetd, Cage and the locker can contend for a VT
  or DRM. Verify release and capability at each boundary and keep the independent text
  boot path. QEMU cannot establish how the Stream's panel and i915 behave on resume.
- **A changed input layout at a password field.** Plymouth's direct input and Cage's
  startup XKB may differ from the kernel console. Test the real protocol paths. Keep the
  current keyboard preview timeout and rollback, and report next-login behavior if
  live preview cannot be made correct under Cage.
- **Boot storage and update churn.** Plymouth adds initrd dependencies. Measure the
  update delta, closure and ESP headroom and preserve the release size constraints.
  Check that Settings changes affect only the small palette tail, with no main initrd
  hash change. Keep the shipped initrd free of release numbers or other changing
  inputs that would needlessly resend it.
- **Wayland coverage.** Capture globals, output scaling, hotplug, cursor planes and
  full IME support vary. Qualify what the packaged compositor exposes. Compose and
  input correctness are required. Advertise IME only after matching compositor
  text-input/input-method support is tested.

## Questions for the owner

1. **Which greeter ships?** Keep native and text for rc34. Revisit a separately owned
   Wayland greeter after the installed writer has shipped.
2. **How should locking coexist with Cage?** Keep the authenticated tty8 VT handoff.
   Keep Wayland opt-in if the existing handoff cannot be qualified without changing
   lock or suspend policy.
3. **Which Wayland renderer ships?** Use the current software Unifont raster via
   `wl_shm`. Keep wgpu for the later GPU-rendering decision and comparisons.
4. **What stays installed, and for how long?** Keep native DRM, Foot and wgpu as rc34
   diagnostics. Review them at rc35. Keep the kernel-console TUI permanently and keep
   the live ISO native/console during rc34 to preserve installer access.
5. **Which disk unlock path ships?** Keep systemd stage 1. Let cryptsetup use Plymouth
   for graphical prompting once all four layouts work with the real password agents.
   Use the existing systemd text path for recovery and where graphical input is unsafe.
6. **Who gets the changed boot palette?** Apply the latest choice to every compatible,
   retained installed generation. Each generation keeps its own firmware.
7. **How should the two display trials graduate?** Ship sunsetr warmth after the Stream
   trial with neutral day on fresh installs and a day-temperature setting. Keep wluma
   experimental and off unless explicitly enabled, with separate webcam consent.
