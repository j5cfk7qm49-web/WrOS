# Ratatui from power-on to the login screen (#89, #86)

The owner's goal (2026-11-07): the whole start, from the logo to the Lemurs-style
login, is drawn with Ratatui, including the disk password. The disk password is
the risky step, so every part has a way back to Debian's own prompt, and each is
tested before it reaches the laptop.

This replaces the Plymouth theme in [BOOT_SCREEN](BOOT_SCREEN.md), which is
removed once the Ratatui screens pass on the laptop.

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
screen just "WrOS". `crates/wros-boot/src/logo.rs` holds it as typed.

ratatui-splash-screen draws pictures with Braille dots, which the console's font
doesn't have, so the start screens don't use it; a character logo needs no
picture. A fade-in can come from tachyonfx (rc10).

**Theme.** Before the unlock the writer's settings are on the encrypted disk, so
the start screens read a root-owned copy of the active theme,
`/etc/wros/boot-theme.toml`, in the same format as `~/.config/grimoire/theme.toml`.
The hook copies it into the boot image; without it the default theme is used.
Changing theme in Settings will update the copy and rebuild the boot image in
the background (to build).

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
3. After the laptop passes its checklist, the update turns the switch on by
   default. Settings › System › Display gets a choice: WrOS start screen or plain
   text.

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
4. `wros-boot splash` with systemd's progress, handing over to greetd; the
   **WrOS (text start)** boot entry.
5. The logo in `wros-greeter`, and the theme copy kept up to date from Settings.
6. Laptop checklist; then the default switch and the Settings choice; then remove
   the Plymouth theme.
