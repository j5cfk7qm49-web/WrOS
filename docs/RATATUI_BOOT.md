# Ratatui from power-on to the login screen (#89, #86)

The owner's goal (2026-11-07): the whole start, from the logo to the Lemurs-style
login, is drawn with Ratatui, including the disk password. The disk password is
the risky step, so every part has a way back to Debian's own prompt, and each is
tested before it reaches the laptop.

This replaces the Plymouth theme. The switch stays off by default until the
owner has passed the laptop checklist.

## What the writer sees

1. **Logo.** From the moment the kernel hands over, the WrOS logo, drawn by Ratatui.
2. **Disk password.** Under the logo: "Type your disk password, then press Enter",
   a dot per key, and after a wrong try "That password didn't unlock the disk
   (try 2 of 3)". Esc switches to Debian's plain text prompt.
3. **Starting.** After the unlock, the logo with a Ratatui progress bar fed by
   systemd's own boot progress.
4. **Login.** The Lemurs-style login (`wros-greeter`), with the same logo above the
   form, so the screens follow on without a jump.

## The pieces

### `wros-boot` (new crate)

One small binary with two modes:

- `wros-boot unlock --prompt TEXT`: the disk password screen. It draws on the
  console (`/dev/console`), reads keys from it in raw mode, and writes the password
  to standard output once, at the end, which is how Debian's `askpass` hands it to
  cryptsetup. It never writes the password anywhere else and wipes it from memory.
- `wros-boot splash`: the logo and progress bar after the unlock.

Its own key reader, not crossterm's: in the boot image there is no terminal type
to query, so it reads bytes, takes Enter, Backspace, Ctrl-U (clear), Esc (text
prompt) and printable characters, and skips arrow-key sequences. The kernel's
keyboard layout applies to those bytes exactly as it does for Debian's prompt, so
Colemak behaves the same in both.

**Logo.** The owner's design (2026-11-07): W, r, O and S in four boxes, drawn
with the characters the owner typed (`#` for the letters, `+ - |` for the boxes),
in the active theme's accent and border colours. Plain ASCII is in every console
font, so it looks the same before the unlock as after. It is 35 columns by 17
rows; below that, a half-size copy of the same letters is used, and on a tiny
screen just "WrOS". `crates/wros-shell/src/logo.rs` holds it as typed, shared by
the unlock, progress and login screens. The Lemurs form stays below it, with a
smaller logo on a 25-line console so all the fields remain visible.

ratatui-splash-screen draws pictures with Braille dots, which the console's font
doesn't have, so the start screens don't use it; a character logo needs no
picture. A fade-in can come from tachyonfx (rc10).

**Theme.** Before the unlock the writer's settings are on the encrypted disk, so
the start screens read a root-owned copy of the active theme,
`/etc/wros/boot-theme.toml`, in the same format as `~/.config/grimoire/theme.toml`.
The hook copies it into the boot image; without it the default theme is used.
Accepting a preset or saving a custom theme in Settings updates the copy through
`wros-boot-settings`, authorized by polkit for an active, local member of `wros`.
It accepts only bounded theme text and a fixed choice, never a user file path.
Settings says: "The start screen takes the new colours at the next restart."
After an update, the first writing session carries over an existing theme too;
later sessions leave an identical copy alone.

Settings › System › Display › **Start screen** chooses **WrOS** or **plain text**.
The same helper writes or removes the switch and queues a boot-image rebuild.
The packaged `wros-boot-rebuild.path` starts the background service; changes made
during a build cause a second build. An interrupted build is retried when the
path unit starts again. Settings reports preparation and a failed rebuild; choose
**Prepare the start screen again** to retry. Reading the choice does not enable it.

### Boot image hook

A hook in `/usr/share/initramfs-tools/hooks/wros-boot`, run after cryptsetup's
(`PREREQ="cryptroot"`), copies `wros-boot` into the boot image and wraps the boot
image's `askpass`:

```sh
/usr/lib/cryptsetup/askpass          WrOS wrapper (boot image only)
/usr/lib/cryptsetup/askpass.debian   Debian's own prompt, unchanged
```

The installed system's `askpass`, crypttab and cryptsetup are not touched.

## Fallbacks

Each failure ends at Debian's own text prompt, never at a locked disk:

| Situation | What happens |
| --- | --- |
| `wros.unlock=text` on the kernel line | The wrapper runs Debian's prompt directly |
| The **WrOS recovery** boot entry | Has `wros.unlock=text` (built); its messages stay visible |
| A new boot menu entry, **WrOS (text start)** | Same, for an ordinary start |
| Esc at the Ratatui prompt | `wros-boot` exits with "text, please"; the wrapper runs Debian's prompt |
| `wros-boot` missing, not executable, or its libraries missing | The wrapper runs Debian's prompt |
| `wros-boot` crashes, can't open the console, or the screen is too small | It exits without writing anything; the wrapper runs Debian's prompt |
| The hook can't build the boot image with `wros-boot` | The hook leaves Debian's `askpass` in place and says so; `update-initramfs` still succeeds |
| A wrong password | Counts as one of cryptsetup's tries, exactly as with Debian's prompt |

Two rules make these safe: `wros-boot` writes the password to standard output only
after Enter, in one write, so a crash can never send half a password; and the
wrapper only skips Debian's prompt when `wros-boot` exits 0.

Lost with the Ratatui prompt, and kept by the text prompt: unlocking over the
network (dropbear's `passfifo`) and systemd's password agents. WrOS uses neither.

## Rollout

1. Build and test everything below on the test computer.
2. On the laptop the hook first runs behind a switch, off by default:
   `/etc/wros/boot-screen` containing `ratatui`. With the switch off the boot image
   is exactly Debian's.
3. Settings offers WrOS or plain text now. The switch remains off on a new
   install; changing that default needs the owner's decision after the checklist.

## Tests

**Unit tests (`wros-boot`):**
- the key reader: characters, UTF-8, Backspace across multi-byte characters,
  Ctrl-U, Enter, Esc alone against Esc that starts an arrow sequence, and a full
  buffer;
- the password goes to standard output once, only after Enter, and never into the
  drawn screen (each frame is checked for the typed text);
- the try counter from `CRYPTTAB_TRIED`;
- the screen at console sizes (80×25, 100×37, 128×48, 170×48) as insta snapshots;
- the half-block logo against its source picture.

**Terminal tests:** the real binary in a pseudo-terminal (vt100): type, Backspace,
Enter, and the password arrives on standard output; Esc exits with "text, please";
a closed terminal exits without output.

**Hook and wrapper tests**, in a fake boot image folder:
- the hook copies `wros-boot`, keeps `askpass.debian` and installs the wrapper;
- with the switch off, it changes nothing;
- the wrapper runs Debian's prompt for `wros.unlock=text`, a missing binary, a
  failing binary, a crashing binary, and Esc, and passes the password through
  unchanged on success.

**Test computer (VM), on an encrypted install:**
- unlock with the Ratatui prompt; then a wrong password, then the right one;
- three wrong passwords end where Debian's prompt ends;
- Esc, then the text prompt unlocks;
- `wros.unlock=text`, the **WrOS (text start)** entry and the recovery entry use
  the text prompt;
- a deliberately broken `wros-boot` in the boot image still unlocks through the
  text prompt;
- screenshots of each screen (QEMU), to see what the console font really draws;
- the progress screen hands over to the login without a flash of text;
- an update from rc7, and going back a version, both leave a working unlock.

**Laptop checklist:** Colemak password at the Ratatui prompt, a wrong one first,
Esc to the text prompt, the text-start entry, and timing on the Celeron.

## Order of work

1. **Built (rc9):** `wros-boot unlock` with its key reader and screen; 12 unit
   tests and 5 terminal tests.
2. **Built (rc9):** the hook and wrapper, with the switch (off), the recovery
   entry's `wros.unlock=text`, and packaging; 10 hook and wrapper tests.
3. VM tests on an encrypted install, including the broken-binary case, with
   QEMU screenshots of the console.
4. **Built (rc9):** `wros-boot splash` with systemd's progress, as
   `wros-boot-splash.service`, stopped just before greetd takes tty1; not on a
   `wros.unlock=text` start. **Built for rc10:** the **WrOS (text start)** entry
   next to recovery, with the normal login and Debian's unlock prompt.
5. **Built for rc10:** the shared login logo, theme helper, background rebuild
   and Settings choice. The Plymouth theme and its packaging are removed.
6. Laptop checklist. The owner decides whether the switch may become the default.
