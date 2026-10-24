# WritingDeck OS

WritingDeck OS is an offline-first Debian writing appliance built around Grimoire. It is an independent project, not endorsed by NaNoWriMo. This is a development foundation, not a complete production OS.

## WritingDeck dashboard

Press `F12`, or choose WritingDeck dashboard in the menu. Resume writing returns to the manuscript. Daily and novel goals, weekly progress, sprints, recent scenes and available system status use real Grimoire data. Local saving and local history are not off-device backup.

Arrow keys select a row. Enter opens it. On a widget, `h` hides it and `J` or `K` moves it. Configure widgets restores hidden widgets. All choices are saved outside manuscript text.

## Toggle outline

`F10` shows or hides the left outline column without entering focus mode. `Ctrl-D` is strict focus: reference panes, outline, scenery and music controls disappear. Their documents stay open and return afterward. Tab never focuses a hidden pane.

## Workspaces

Workspaces save the primary document, cursor, scroll position, outline width and focus setting. Choose Drafting, Planning, Research, Revision or Focus, or press `n` to clone a custom layout. Use `+` and `-` to adjust outline width. The initial engine does not yet provide sticky cards or a second editable buffer; existing beside views remain read-only.

## System settings

System settings uses a background worker and cached status. NetworkManager controls Wi-Fi radio and scanning; brightnessctl controls supported backlights. Power controls require confirmation and successful manuscript saving. Unsupported services report unavailable, never success. Connecting a new Wi-Fi network still uses `nmtui` from the maintenance console.

Keyboard layout selects QWERTY, Colemak or Colemak-DH ANSI/ISO on appliance tty1. A root-owned helper previews for 15 seconds with an independent systemd rollback timer. Type in the preview field; Enter keeps it, Escape reverts. The test field never changes a manuscript. Desktop and SSH sessions must use their own OS layout configuration.

Keyboard shortcuts adds aliases to actions using Ctrl+Alt letters or F10-F12. Established Grimoire bindings are protected. Shortcuts use logical letters: Colemak text is not remapped again. Plain letters always remain prose.

Low-power mode reduces redraws and disables the visualizer; requesting an OS power profile can fail independently. Start-at-dashboard is optional; disable it to restore the selected workspace directly.

## Interactive tutorial

Interactive tutorial lists all 39 required lessons. Planned lessons are explicitly unavailable. Press `d` for a new safe demonstration project: your novel saves first and is not overwritten. Enter opens a lesson, then launches the actual feature. Progress is verified by application events; a Next button cannot fake completion. Writing exercises only complete in the demo project. `s` skips a lesson; any implemented lesson can be replayed. Help is searchable offline with F1.

## Safety and maintenance

The writing application never needs root. Ctrl+Alt+F2 opens an authenticated maintenance console on the appliance. The isolated VM prototype has passwordless tty1 autologin and must not be deployed to hardware. Use a normal Debian account and encrypted installation for physical laptops. Screen locking, a full installer, updates/rollback automation, cloud OAuth and real-hardware verification remain release gates.
