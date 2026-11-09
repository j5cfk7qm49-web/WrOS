# Ratatui projects that fit WrOS

**Owner decision, 2026-11-07:** graphical rendering is now the default for disk
unlock, loading, login and writing. The native software backend is integrated
throughout those screens, with an explicit text recovery path. This supersedes
the earlier off-by-default trial policy below; the three-backend comparison and
the remaining rc10 libraries are still separate work. See [Graphical screens](GRAPHICAL_SESSION.md).

Read through the whole [Awesome Ratatui](https://github.com/ratatui/awesome-ratatui)
list (about 560 entries) on 2026-11-06, looking for projects that suit a writing
laptop: an HP Stream (Celeron, 4 GB, 32 GB eMMC), offline most of the time,
NetworkManager for Wi-Fi, ALSA for sound, Colemak, and soon a graphical session
(cage + foot) beside the console one. Licence, last update and stars were checked
on GitHub or crates.io the same day.

Two projects on the list are already part of WrOS: **Grimoire**
([GrimoireTUI](https://github.com/kelsierbot/GrimoireTUI), MIT), the writing
app's upstream, and **Lemurs** (MIT or Apache-2.0), whose screen is coming into
the WrOS login (#86).

## How they can join WrOS

- **MIT or Apache libraries** can be built into the writing app, the login
  screen and Settings, like any other crate. They must match our ratatui
  version (0.30).
- **GPL or AGPL programs** can ship as separate WrOS programs that Settings
  opens, but their code cannot be merged into the MIT writing app.
- **Projects with no licence** cannot be used at all.

## Libraries to build in

| Project | Licence | What it would do in WrOS |
| --- | --- | --- |
| [tachyonfx](https://github.com/ratatui/tachyonfx) | MIT | Fades and transitions: login to writing, the lock screen, tutorial highlights. Off in low-power mode. Kept by the Ratatui project. |
| [ratatui-image](https://github.com/ratatui/ratatui-image) | MIT | Pictures in the graphical session (foot shows Sixel images): book covers, picture notes on the pin board, character portraits. The console session falls back to coloured blocks. |
| [tui-widgets](https://github.com/ratatui/tui-widgets) | Apache-2.0 | A set from Ratatui's maintainers. **tui-big-text** for a large clock on the login and lock screens and chapter title cards; **tui-popup**, **tui-scrollview** and **tui-prompts** to replace hand-made versions. |
| [termprofile](https://github.com/aschey/termprofile) | MIT or Apache-2.0 | Knows how many colours the screen can show. The console has 16 and foot has millions, so one WrOS theme can look right in both sessions. Needed for rc10. |
| [opaline](https://github.com/hyperb1iss/opaline) | MIT | A theme engine with 20 built-in themes and a theme picker widget, for a Themes page in Settings. |
| [ratatui-splash-screen](https://github.com/orhun/ratatui-splash-screen) | Apache-2.0 | A WrOS picture while the laptop starts. |
| [taria-ratatui](https://github.com/y0sif/taria) | MIT or Apache-2.0 | Publishes the app's screen and actions so a test program can press the right buttons by name. Laptop tests without screen-scraping or a webcam. |
| [tui-pantry](https://github.com/taho-inc/tui-pantry) | MIT or Apache-2.0 | Shows every widget in every state on one page, for checking the look of both sessions. |

## Programs that could become WrOS pages

| Project | Licence | Fit |
| --- | --- | --- |
| [wtfi2](https://github.com/kanywst/wtfi2) | MIT | Finds where a Wi-Fi connection breaks (link, router, DNS, internet) and can record a day of drops. Linux and macOS. A "Why is Wi-Fi not working?" button on the Wi-Fi page, and the day-long Wi-Fi check in the hardware checklist. |
| [dua-cli](https://github.com/Byron/dua-cli) | MIT | Shows what fills the 32 GB disk and deletes safely. A Storage page. |
| [bluetui](https://github.com/pythops/bluetui) | GPL-3.0 | Bluetooth keyboards and headphones through BlueZ. A separate program. |
| [kbt](https://github.com/bloznelis/kbt) | MIT | Keyboard tester. Helps with the keyboard wizard and the hardware checklist. Last updated a year ago. |
| [systemctl-tui](https://github.com/rgwood/systemctl-tui), [journalview](https://github.com/codervijo/journalview) | MIT | Services and system logs from the recovery menu, so problems can be read without SSH. |
| [vib](https://github.com/ayanchavand/vib) | MIT | Sends files with LocalSend over the local Wi-Fi: exports to the writer's phone without Google Drive. |

Not a fit: **impala** (Wi-Fi, but for iwd, not NetworkManager), **serenIT**
(ambient sounds, no licence), **tmmpr** (mind maps, archived).

## For writers

| Project | Licence | Fit |
| --- | --- | --- |
| [doxx](https://github.com/bgreenwell/doxx) | MIT | Opens Word documents in the terminal. Read the .docx an editor or agent sends back. Strong fit, well used. |
| [readio](https://github.com/hrhrng/readio) | MIT | Ebook and Markdown reader with local text-to-speech: hear the draft read back to catch mistakes. Small project. |
| [ostt](https://github.com/kristoferlund/ostt) | MIT | Local speech-to-text with a volume meter: dictation. Speed on the Celeron needs a test. |
| [herdfold](https://github.com/azihsoyn/herdfold) | Apache-2.0 | Lays long text out as facing book pages: proofread the manuscript as a book. Very new; an idea to borrow more than a dependency. |
| [writui](https://github.com/aakashns/writui) | MIT | Markdown writing app with an encrypted vault and named versions. Ideas for Grimoire. |
| [ratslate](https://github.com/azihsoyn/ratslate) | Apache-2.0 | Infinite canvas whiteboard. Ideas for the pin board. |
| [bookokrat](https://github.com/bugzmanov/bookokrat) | AGPL-3.0 | EPUB and PDF reader. A separate program only. |
| [keybr-tui](https://github.com/y0sif/keybr-tui) | AGPL-3.0 | Typing practice that finds slow keys, useful while learning Colemak. A separate program only. |

## For rc10: drawing without a terminal

[soft_ratatui](https://github.com/gold-silver-copper/soft_ratatui),
[ratatui-wgpu](https://github.com/Jesterhearts/ratatui-wgpu) and
[raclettui](https://github.com/ishrut/raclettui) (all Apache-2.0, all on
ratatui 0.30) draw a ratatui app into pixels instead of a terminal. Under cage,
the writing app could draw itself, with no foot in between.

| | soft_ratatui | ratatui-wgpu | raclettui |
| --- | --- | --- | --- |
| Draws with | The processor, into a picture in memory | The graphics chip (wgpu, Vulkan through Mesa) | Either |
| Opens a window | No: something else must show the picture | Yes, through winit | Only layer-shell windows (bars, panels). Cage 0.2.0 offers no layer shell (checked headless, 2026-11-06), so it cannot run under cage as it is. |
| Keyboard | Not included | winit's, which uses xkbcommon | Its own |
| Fonts | Bitmap fonts, TrueType, or cosmic-text shaping | rustybuzz shaping, bidi, combining marks | fontdue; bold and italic only on the graphics chip |
| Maturity | 106 stars, small | 135 stars, active | 46 stars, "expect bugs and breaking changes" |

**Compared with foot:**

- **For:** one program fewer, so a smaller update and fewer settings; the
  app gets every key directly (no terminal escape codes, no kitty keyboard
  protocol, key releases included); text, colours and pictures drawn exactly
  as the app wants, with images drawn straight from the app instead of
  through Sixel; screenshots for tests come straight from the app, even without
  cage or a screen (soft_ratatui renders to a picture in memory); effects
  such as tachyonfx can go further with shaders (ratatui-wgpu).
- **Against:** foot is a Debian package with security updates and years of
  use; these are small projects we would partly maintain. The app would own
  the Wayland window, key repeat, Colemak through xkbcommon, compose and dead
  keys, the clipboard, scaling and resizing, which foot does now. The input
  reader (`wros-input`) and the console lock are built around a terminal, so
  the graphical session's input path would be new code to secure and test.
  On the HP Stream, the graphics chip path adds wgpu and Vulkan memory and
  start time and depends on Mesa's Intel driver; the processor path redraws
  more than foot, which only redraws what changed.
- **Either way** the console session keeps the terminal backend, so the app
  would carry two backends.

**Trial for rc10 (decided 2026-11-06):** three ways to show the writing app,
compared on the HP Stream for start time, typing delay, memory, disk space,
Colemak and compose keys, and a day of writing. If none is good enough, WrOS
stays on the console session.

1. **foot under cage** (the rc9 preview). Nothing new to install or write.
2. **ratatui-wgpu under cage.** foot goes; the app opens its own window and
   reads keys through winit and xkbcommon. Try wgpu's OpenGL mode first, which
   uses the Mesa drivers cage already brings; Debian's Vulkan driver
   (`mesa-vulkan-drivers`, 81 MB) is not installed by cage.
3. **soft_ratatui straight to the screen.** No cage, no foot, no Mesa. The
   app renders to pixels and shows them through the kernel's display
   interface (DRM/KMS, as the warm-screen helper already uses), reading keys
   through libinput and xkbcommon on its own console. The most new code:
   taking the display, key repeat, compose keys and the clipboard are ours.
   The console lock and the warm screen stay close to how they work today.

Not in the trial: Alacritty (larger than foot, no pictures), kitty (heavy, needs
Python), and terminals Debian does not ship (WezTerm, Ghostty, Rio, kmscon).

## Decided for rc10 (2026-11-06)

**tachyonfx**, **tui-widgets**, **ratatui-splash-screen** and **taria-ratatui**
go into rc10 with the graphical session. The drawing backends above get the
trial against foot.

The shared **tui-big-text** clock is implemented for login and lock. Login shows
local time beside the shared logo, leaving the full form visible at 80 by 25.
Smaller layouts keep the form and omit the clock. The clock uses full-block
characters supported by both the graphical font and installed Linux console;
compressed half-block modes failed the installed-font screenshot check (#105).
The lock shows **Locked at HH:MM**, a static lock-start time prepared once by
the packaged greeter's write-only message mode. Debian vlock/PAM alone reads
unlock credentials; no background clock redraw runs during authentication.
If the message renderer is unavailable, the original lock message still works.
The clock has no editor or authentication dependency. Its dependency notices
are retained with the package. [Clock evidence](evidence/rc10-clocks.json)
records the checks and limits.

**TachyonFX** fades the first writing frame over 210 ms and tutorial highlights
over 150 ms. The writing loop continues to handle keys during both effects.
After retiring writing input, the graphical lock shows a 150 ms fade on a
simple **Locking…** card, then hands the screen and keyboard to Debian's locker.
Console and low-power sessions keep static colours. [Transition evidence](evidence/rc10-motion.md)

**ratatui-splash-screen** shows the four-box WrOS logo in Braille dots as the
graphical writing session opens. A key or paste can skip the brief picture and
still reach the writer. The console and low-power sessions open without the
picture. [Startup picture evidence](evidence/rc10-startup-picture.md)

**taria-ratatui** publishes menu rows and dialog controls in debug test builds.
The lock handoff shows only a status. On return the test client reconnects, so
actions queued during authentication cannot reach the writer. The PTY fixture
checks this alongside the separate credential and physical-input tests.
[Test driver](TESTING.md#the-app-in-a-terminal-driven-by-name)

## Suggested order for the rest

1. termprofile with the graphical session (same look in both sessions).
2. ratatui-image with the graphical session.
3. wtfi2 and dua-cli as Settings pages; doxx for reading Word files.
4. Speech (readio, ostt) after a speed test on the laptop.
