# QWERTY, Colemak and Colemak-DH

## What is working

The appliance has first-class registered console maps for QWERTY (US), Colemak, Colemak-DH ANSI and Colemak-DH ISO. It compiles XKB `us`, `us(colemak)`, `us(colemak_dh)` and `us(colemak_dh_iso)` from pinned nixpkgs through console-setup's `ckbcomp -model pc105`. `loadkeys --unicode --bkeymap` validates output without opening a real console. The DH physical E → logical F result is a retained Debian-era VM report. The NixOS keyboard-preview test covers timeout rollback, confirmation, persistence and encrypted reboot with Colemak. This describes coverage, not a physical-keyboard result.

Use **Settings › Look and keyboard › Keyboard › Keyboard layout**. Arrow keys select; Enter starts a temporary preview. Type in the isolated test field. Enter within the deadline keeps the layout; Escape cancels. An independent systemd timer restores the committed layout after 15 seconds even if Grimoire crashes. The helper is authoritative about expiry; the UI countdown is informational. The test field never edits a manuscript.

wros-keyboard.service applies the committed map after systemd-vconsole-setup and before greetd. Confirmed layouts are also copied to /boot/wros-boot/keyboard.json for disk unlock. Do not also install an application letter-to-letter translator. Layout IDs in settings are physical-system configuration, not instructions to change already translated manuscript characters.

The NixOS installer selects a registered layout and attempts to switch its keyboard before password entry. If loading the map fails, it continues with the previous layout. It writes the choice to /etc/wros/keyboard.json and /boot/wros-boot/keyboard.json for login and disk unlock. Updates preserve the mutable committed choice. [Debian installer keyboard history](archive/INSTALLER_KEYBOARD.md) records the earlier console-setup handoff.

## Important scope

This switcher targets an installed WrOS **Linux virtual console tty1**. It does not change a remote SSH client's keyboard or a conventional desktop terminal's XKB/Wayland configuration. Use the desktop compositor's supported settings or the SSH client's OS in those cases. Changing loadkeys in a desktop terminal is not the correct fix.

The baseline application already normalizes AltGr, receives Unicode and terminal escape sequences, and uses logical-character shortcuts. New aliases protect existing bindings and never intercept unmodified prose letters. Raw physical-key shortcuts cannot be implemented reliably through a generic Linux tty byte stream; do not infer hardware positions from characters. Function keys/arrow keys offer layout-independent navigation.

ANSI and ISO differ physically, particularly around the lower-left extra key. Choose the matching variant; ortholinear, locale-specific and administrator-registered custom layouts need explicit new registration/testing. Runtime arbitrary keymap paths are intentionally rejected. The first-run wizard and installer layout UI support the four registered layouts. The wizard can register an alias for a supported base layout; arbitrary keymap compilation remains disabled.

## Recovery

Wait for timeout if a preview is wrong. Escape and Enter are position-stable. If the app has stopped, the systemd timer still owns rollback. If a deliberately committed layout is wrong, use the authenticated maintenance console/admin recovery path to select QWERTY. Reboot only restores the committed choice, not necessarily QWERTY. Never remove the known-good QWERTY map.

Historical Debian prototype warning: do not deploy its passwordless VM profile. Current NixOS WrOS uses an installer-created writer account, locked root and authenticated maintenance/recovery.

## Learning and printable reference

[Printable keyboard reference](keyboard-reference.html) is self-contained HTML of the logical shortcut bindings, including F10 for the menu bar and F11 for the independent editor, and the current keyboard settings paths. It names supported layouts but does not certify physical ANSI/ISO keyboard diagrams. Print it from a normal browser, or copy it to another machine; no external fonts/network resources are needed. Interactive tutorial lesson 2 opens the actual safe preview, and lesson 10 uses logical-letter shortcut aliases. F11 and arrows/Tab also operate the independent editors without letter remapping. All tutorial instructions use arrows/function keys for discovery rather than assuming QWERTY mnemonic positions.

Colemak Mod-DH reference: <https://github.com/ColemakMods/mod-dh> (CC0-1.0). No repository keymap was copied; the image uses maintained XKB/console-setup and retains their Debian copyright notices. Standard Colemak's Caps Lock behavior comes from that upstream map; verify your physical modifiers before normal writing.

## Required further verification

Physical ANSI/ISO laptops: Shift/Ctrl/Alt/AltGr, Caps/Backspace, F1–F12 sequences, Unicode/compose input, escape behavior, cold boot, suspend/resume, tty2 recovery and switching under a desktop. Logical-input TUI tests are **not** physical-layout verification. Custom layouts require administrator review plus a fixed-ID registration workflow before the helper can load them.
