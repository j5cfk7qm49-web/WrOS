# WritingDeck OS

WritingDeck OS is an offline-first Debian writing appliance built around Grimoire. It is an independent project, not endorsed by NaNoWriMo. This is a development foundation, not a complete production OS.

## WritingDeck dashboard

Press `F12`, or choose WritingDeck dashboard in the menu. Resume writing returns to the manuscript. Daily and novel goals, weekly progress, sprints, recent scenes and available system status use real Grimoire data. Local saving and local history are not off-device backup.

Arrow keys select a row. Enter opens it. On a widget, `h` hides it and `J` or `K` moves it. Configure widgets restores hidden widgets. All choices are saved outside manuscript text.

## Toggle outline

`F10` shows or hides the left outline column without entering focus mode. `Ctrl-D` is strict focus: reference panes, outline, scenery and music controls disappear. Their documents stay open and return afterward. Tab never focuses a hidden pane.

## Workspaces

Workspaces save both documents, each cursor/selection/scroll position, the active editor, secondary visibility/proportion, outline width and focus setting. Choose Drafting, Planning, Research, Revision or Focus, or press `n` to clone a custom layout. Use `+` and `-` to adjust outline width and `<` or `>` for the secondary proportion (25–75 percent). Single-source sticky cards are now available through Pinboard; explicit starter pane content and workspace-specific pin sets are now available: in Workspaces press b for a starter or p to choose card IDs. Starters create an empty ordinary original note only on explicit application, never while loading/selecting a workspace or replacing existing prose. The legacy `v` beside view is still read-only, separate from the new editor.

## Toggle editable secondary pane

Press `F11` to show or hide the independent editor. The first press opens Choose secondary document: type a scene, character, note or research title and press Enter. The same action in Writing tools or Ctrl-K replaces its document safely. Both panes are fully editable, each with its own undo/redo, cursor, selection and scroll. Clipboard, find/replace and spelling commands target the focused buffer; the clipboard and dictionary are shared. Tab or a click changes the focused editor; commands act on that buffer. Hiding never discards it. At 80×24 the two editors stack; wider writing areas put them side by side.

Only one authoritative editor may own a canonical file. Choosing a document already open focuses its existing pane, including symlink/hard-link aliases on Linux. Both buffers use the existing autosave/history/conflict/recovery path and raw YAML frontmatter. Strict focus hides the secondary; leave with Ctrl-D before using F11. A secondary and an old read-only reference are retained but not drawn simultaneously. No file is copied merely to open another view.

## Sticky-note pins

Choose Pinboard or Pin current document in Writing tools or Ctrl-K. Dashboard `p` opens Pinboard too. Cards reference existing Markdown sources, keep independent scroll and heading/quote anchors, and never become a competing text store. Enter explicitly edits a source through the existing owner-aware editor. Scopes, category/color/size/order, recoverable card archive/Trash and narrow-screen fallback are explained in the Sticky-note pins help topic. Writing journal now separates sampled saved-Project facts from original Markdown reflections: Ctrl-K or dashboard j opens it. Its help explains source editing, explicit current snapshots and retry. Scratchpad now has multiple original Notes, explicit scene links and original-preserving scene promotion; Session handoff shows an ordinary user-authored next-open intention. Dashboard s/o and their own help topics expose both.

## Unified WrOS settings

From the dashboard choose Settings, or use the command palette's WrOS settings
action. Writing, Interface, System and Accounts share arrow/Enter/Escape
navigation and remember their selections. Existing editor and interface actions
use the same underlying settings. Closing a child dialog returns to its category;
Escape follows the route back to the writing workspace. Both editable documents,
cursor, selection, free scroll and undo remain retained. F1 opens offline help.
The older Settings menu remains available during this migration.

System has native Network, Display, Power, Storage and Date/time panes using
cached background services. Accounts, updates and recovery explicitly identify
unfinished services. Display → Warm screen has an offline filter on supported
drivers: 1000–6500 K, initial 2200 K, strength/Off and a 15-second Keep/Cancel
preview. Off restores original color curves. Unsupported drivers are unavailable;
this is separate from the theme picker. No Google account is required for local
writing. Suspend is unavailable until secure screen locking has been qualified.

## System settings

System settings uses a background worker and cached status. NetworkManager controls Wi-Fi radio and scanning; brightnessctl controls supported backlights. Power controls require confirmation and successful manuscript saving. Unsupported services report unavailable, never success. Native connection enrollment is still unfinished. `nmtui` remains available
only as an emergency maintenance tool while that work is completed.

Keyboard layout selects QWERTY, Colemak or Colemak-DH ANSI/ISO on appliance tty1. A root-owned helper previews for 15 seconds with an independent systemd rollback timer. Type in the preview field; Enter keeps it, Escape reverts. The test field never changes a manuscript. Desktop and SSH sessions must use their own OS layout configuration.

Keyboard shortcuts adds aliases to actions using Ctrl+Alt letters or F10-F12. Established Grimoire bindings are protected. Shortcuts use logical letters: Colemak text is not remapped again. Plain letters always remain prose.

Low-power mode reduces redraws and disables the visualizer; requesting an OS power profile can fail independently. Start-at-dashboard is optional; disable it to restore the selected workspace directly.

## Interactive tutorial

Interactive tutorial lists all 39 required lessons. Planned lessons are explicitly unavailable. Press `d` for a new safe demonstration project: your novel saves first and is not overwritten. Enter opens a lesson, then launches the actual feature. Progress is verified by application events; a Next button cannot fake completion. Writing exercises only complete in the demo project. `s` skips a lesson; any implemented lesson can be replayed. Help is searchable offline with F1.

## Safety and maintenance

The writing application never needs root. Ctrl+Alt+F2 opens an authenticated maintenance console on the appliance. The isolated VM prototype has passwordless tty1 autologin and must not be deployed to hardware. Use a normal Debian account and encrypted installation for physical laptops. Screen locking, a full installer, updates/rollback automation, cloud OAuth and real-hardware verification remain release gates.
