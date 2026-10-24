# QWERTY, Colemak and Colemak-DH

## What is working

The appliance has first-class registered console maps for QWERTY (US), Colemak, Colemak-DH ANSI and Colemak-DH ISO. It compiles Debian XKB `us`, `us(colemak)`, `us(colemak_dh)` and `us(colemak_dh_iso)` through `ckbcomp -model pc105`. `loadkeys --unicode --bkeymap` validates output without opening a real console. The actual VM has verified DH physical E → logical F, timed revert and committed-map reapplication.

Use **F12 → System settings → Keyboard layout**. Arrow keys select; Enter starts a temporary preview. Type in the isolated test field. Enter within the deadline keeps the layout; Escape cancels. An independent systemd timer restores the committed layout after 15 seconds even if Grimoire crashes. The helper is authoritative about expiry; the UI countdown is informational. The test field never edits a manuscript.

The committed map is applied after Debian console-setup and before tty1 login. Do not also install an application letter-to-letter translator. Layout IDs in settings are physical-system configuration, not instructions to change already translated manuscript characters.

## Important scope

This switcher targets an installed WritingDeck **Linux virtual console tty1**. It does not change a remote SSH client's keyboard or a conventional desktop terminal's XKB/Wayland configuration. Use the desktop compositor's supported settings or the SSH client's OS in those cases. Changing loadkeys in a desktop terminal is not the correct fix.

The baseline application already normalizes AltGr, receives Unicode and terminal escape sequences, and uses logical-character shortcuts. New aliases protect existing bindings and never intercept unmodified prose letters. Raw physical-key shortcuts cannot be implemented reliably through a generic Linux tty byte stream; do not infer hardware positions from characters. Function keys/arrow keys offer layout-independent navigation.

ANSI and ISO differ physically, particularly around the lower-left extra key. Choose the matching variant; ortholinear, locale-specific and administrator-registered custom layouts need explicit new registration/testing. Runtime arbitrary keymap paths are intentionally rejected. **Custom layout support and installer/first-run layout UI are not complete.** The read-only installation planner records one of the registered choices but does not apply it to a disk.

## Recovery

Wait for timeout if a preview is wrong. Escape and Enter are position-stable. If the app has stopped, the systemd timer still owns rollback. If a deliberately committed layout is wrong, use the authenticated maintenance console/admin recovery path to select QWERTY. Reboot only restores the committed choice, not necessarily QWERTY. Never remove the known-good QWERTY map.

The prototype's VM passwords are locked; its recovery/login behavior is not the production security model. Do not deploy the passwordless profile to a laptop.

## Learning and printable reference

[Printable keyboard reference](keyboard-reference.html) is self-contained HTML of the actual logical shortcut bindings, including F11. It names supported layouts but does not certify physical ANSI/ISO keyboard diagrams. Print it from a normal browser, or copy it to another machine; no external fonts/network resources are needed. Interactive tutorial lesson 2 opens the actual safe preview, and lesson 10 uses logical-letter shortcut aliases. F11 and arrows/Tab also operate the independent editors without letter remapping. All tutorial instructions use arrows/function keys for discovery rather than assuming QWERTY mnemonic positions.

Colemak Mod-DH reference: <https://github.com/ColemakMods/mod-dh> (CC0-1.0). No repository keymap was copied; the image uses maintained XKB/console-setup and retains their Debian copyright notices. Standard Colemak's Caps Lock behavior comes from that upstream map; verify your physical modifiers before normal writing.

## Required further verification

Physical ANSI/ISO laptops: Shift/Ctrl/Alt/AltGr, Caps/Backspace, F1–F12 sequences, Unicode/compose input, escape behavior, cold boot, suspend/resume, tty2 recovery and switching under a desktop. Logical-input TUI tests are **not** physical-layout verification. Custom layouts require administrator review plus a fixed-ID registration workflow before the helper can load them.
