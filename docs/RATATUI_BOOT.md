# Ratatui from power-on to the login screen (#89, #86)

The owner's goal (2026-11-07): the whole start, from the logo to the Lemurs-style
login, is drawn with Ratatui, including the disk password. The disk password is
the risky step, so every part has a way back to Debian's own prompt, and each is
tested before it reaches the laptop.

This replaces the Plymouth theme. The owner chose graphical defaults for disk
unlock, loading, login and writing on 2026-11-07. These screens now use
`soft_ratatui` with bundled GNU Unifont bitmaps and DRM/KMS scanout.
The kernel keyboard layout still applies, including before disk unlock.

Missing `/etc/wros/boot-screen` now means graphical. An explicit `text` value
disables the unlock and loading screens; `ratatui` enables them. Settings writes
an explicit text value when turned off so a later update preserves the choice.
The **WrOS (text start)** menu entry skips graphics throughout the boot and
writing session. Esc at disk unlock opens systemd-cryptsetup's standard text prompt.

If no supported DRM display is available, the same screens draw on the text
console. A separate `wros-console-reset` restores text mode before
systemd-cryptsetup's standard text prompt after any unsuccessful renderer exit,
including a crash.
It restores line input and Enter's newline conversion, flushes typeahead and
keeps echo off, so a killed raw-mode renderer cannot strand the plain prompt.
NixOS includes the helper's closure in the initrd. `wros-initrd-unlock.service`
hands a usable response to systemd-cryptsetup and leaves its standard text prompt
to handle unlock when no usable key is supplied.

## What the writer sees

1. **Logo.** From the moment the kernel hands over, the WrOS logo, drawn by Ratatui.
2. **Disk password.** Under the logo: "Type your disk password, then press Enter",
   with an asterisk per character that fits. The graphical prompt submits one
   password. A rejected password falls back to systemd's text prompt for further
   attempts; Esc also selects the text prompt.
3. **Starting.** After the unlock, the logo with a Ratatui progress bar fed by
   systemd's own boot progress.
4. **Login.** The Lemurs-style login (`wros-greeter`), with the same logo above the
   form, so the screens follow on without a jump.

## The pieces

### `wros-boot` (new crate)

One small binary with two modes:

- `wros-boot unlock --tty /dev/console --theme /etc/wros/boot-theme.toml`: the disk
  password screen. It draws on the console (`/dev/console`) and reads keys from it
  in raw mode. The binary writes the password to standard output once. The NixOS
  initrd captures it in private ramfs at `/crypt-ramfs/passphrase` for
  systemd-cryptsetup; it is not saved to persistent storage.
- `wros-boot splash`: the logo and progress bar after the unlock.

Its own key reader, not crossterm's: in the boot image there is no terminal type
to query, so it reads bytes, takes Enter, Backspace, Ctrl-U (clear), Esc (text
prompt) and printable characters, and skips arrow-key sequences. The kernel's
keyboard layout applies to those bytes as it does for systemd-cryptsetup's standard
text prompt, so Colemak behaves the same in both.

**Logo.** The owner's design (2026-11-07): W, r, O and S in four boxes, drawn
with the characters the owner typed (`#` for the letters, `+ - |` for the boxes),
in the active theme's accent and border colours. Plain ASCII is in every console
font, so it looks the same before the unlock as after. It is 35 columns by 17
rows; below that, a half-size copy of the same letters is used, and on a tiny
screen just "WrOS". `crates/wros-shell/src/logo.rs` holds it as typed, shared by
the unlock, progress and login screens. The Lemurs form stays below it, with a
smaller logo on a 25-line console so all the fields remain visible.

The start screens use the shared character logo. The graphical Unifont backend
also supports Braille glyphs.

**Theme.** Before the unlock the writer's settings are on the encrypted disk, so
the start screens read a root-owned copy of the active theme,
`/etc/wros/boot-theme.toml`, in the same format as `~/.config/grimoire/theme.toml`.
The preparation service copies the theme to `/boot/wros-boot`; the initrd reads
it from the ESP at boot. Without a saved theme, the default is used.
Accepting a preset or saving a custom theme in Settings updates the copy through
`wros-boot-settings`, authorized by polkit for an active, local member of `wros`.
It accepts only bounded theme text and a fixed choice, never a user file path.
Settings says: "The start screen takes the new colours at the next restart."
After an update, the first writing session carries over an existing theme too;
later sessions leave an identical copy alone.

The text-console fallback adapts a drawing copy of the theme to ANSI-16 through
[termprofile](https://github.com/aschey/termprofile). Graphical unlock, progress
and login keep the RGB theme. The saved theme is unchanged, and conversion never
queries the password terminal.

Settings › System › Display › **Start screen** chooses **WrOS** or **plain text**.
The helper writes the choice and queues preparation. `wros-boot-rebuild.path`
starts a service that synchronises the theme, switch and keyboard files to the
ESP. It does not rebuild the initrd. Settings reports preparation failures and
offers **Prepare the start screen again**.

### `wros-initrd-unlock.service`

The systemd initrd runs `wros-initrd-unlock.service` before
`systemd-cryptsetup@wros_crypt.service`. It reads the theme, switch and keyboard
settings from the ESP read-only, refusing symlinks and limiting each file to
8 KiB. It allows only the compiled keyboard layouts and loads the selected keymap.

The service captures a successful graphical response in private ramfs at
`/crypt-ramfs/passphrase`. systemd-cryptsetup uses that key to open `WROS-LUKS`
as `wros_crypt`. No password is written to persistent storage.
[NixOS boot module](../nixos/modules/boot.nix)

## Fallbacks

When no usable key is supplied, systemd-cryptsetup handles the text prompt:

| Situation | What happens |
| --- | --- |
| `wros.unlock=text` or an explicit text setting | The service skips the graphical prompt |
| **WrOS recovery** or **WrOS (text start)** | The entry supplies `wros.unlock=text` |
| Esc, renderer failure, or a response without a password | The service removes the temporary password and runs `wros-console-reset` |
| A submitted password is rejected | systemd-cryptsetup's standard text prompt handles further attempts |

The service accepts the graphical response only after a successful exit with a
nonempty password file. Renderer failure runs `wros-console-reset` before the
text prompt. Selecting text directly does not need that reset.

## Rollout (Debian rc9/rc10 history)

1. Build and test everything below on the test computer.
2. The owner chose graphical defaults on 2026-11-07. A missing switch, or
   `/etc/wros/boot-screen` containing `ratatui`, enables the graphical boot image.
3. Settings offers WrOS or plain text. An explicit `text` value keeps Debian's
   original image; the text-start boot entry also overrides graphics for one boot.
   The laptop checklist still covers the actual Intel display, timing and power use.

## Tests

Current NixOS checks are [boot-unlock-login](../nixos/tests/boot-unlock-login.nix)
for unlock, authenticated login, ESP settings synchronisation and text start;
[install-encrypted](../nixos/tests/install-encrypted.nix) for installation and reboot;
[recovery-menu](../nixos/tests/recovery-menu.nix) for the recovery specialisation;
and [update-goback](../nixos/tests/update-goback.nix) for generation rollback.
These references describe coverage, not a new pass result. Moving from rc13 or
earlier to NixOS requires reinstalling, not the rc7 update scenario below.
[Current qualification guide](NIXOS_QUALIFICATION.md)

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

**Debian rc9/rc10 history: hook and wrapper tests**, in a fake boot image folder:
- the hook copies `wros-boot`, keeps `askpass.debian` and installs the wrapper;
- a missing switch enables graphics, while a missing or unrunnable recovery
  helper keeps Debian's original prompt;
- with the switch off, it changes nothing;
- the wrapper runs Debian's prompt for `wros.unlock=text`, a missing binary, a
  failing binary, a crashing binary, and Esc, and passes the password through
  unchanged on success.

**Debian rc9/rc10 history: test computer (VM), on an encrypted install:**
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

## Order of work (Debian rc9/rc10 history)

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
6. The owner requested graphical defaults. The laptop checklist still needs
   hardware results; VM qualification does not establish them.
