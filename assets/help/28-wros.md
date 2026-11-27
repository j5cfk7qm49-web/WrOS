# WrOS (Writer's OS)

WrOS (formerly codenamed WritingDeck OS) is an offline-first NixOS 26.05 writing appliance built around Grimoire. It is an independent project, not endorsed by NaNoWriMo. This is a development foundation, not a complete production OS.

## The writing desk

WrOS opens your selected workspace. Press `Esc` for the menu bar or `Ctrl-K`
to find an action or document. Open **Tools › Progress** for goals, calendar,
sprints, edits, collection and challenges. The upper bottom bar shows where
you are and the system widgets. The lower bar shows writing progress and
messages. Save and backup marks sit on the top bar. Esc returns from a WrOS
page to writing or to the page you opened it from.

## Toggle outline

*View › Show the outline* (or *Hide the outline*) shows or hides the left outline column without entering focus mode; it has no key of its own unless you give it one in Keyboard shortcuts. `F10` opens the menu bar. `Ctrl-D` is strict focus: reference panes, outline, scenery and music controls disappear. Their documents stay open and return afterward. Tab never focuses a hidden pane.

## Workspaces

Workspaces save both documents, each cursor/selection/scroll position, the active editor, secondary visibility/proportion, outline width and focus setting. Choose Drafting, Planning, Research, Revision or Focus, or press `n` to clone a custom layout. Use `+` and `-` to adjust outline width and `<` or `>` for the secondary proportion (25–75 percent). Single-source sticky cards are now available through Pinboard; explicit starter pane content and workspace-specific pin sets are now available: in Workspaces press b for a starter or p to choose card IDs. Starters create an empty ordinary original note only on explicit application, never while loading/selecting a workspace or replacing existing prose. The legacy `v` beside view is still read-only, separate from the new editor.

## Toggle editable secondary pane

Press `F11` to show or hide the independent editor. The first press opens Choose secondary document: type a scene, character, note or research title and press Enter. To replace its document safely, use Ctrl-K or *View › Choose second document…*. Both panes are fully editable, each with its own undo/redo, cursor, selection and scroll. Clipboard, find/replace and spelling commands target the focused buffer; the clipboard and dictionary are shared. Tab or a click changes the focused editor; commands act on that buffer. Hiding never discards it. At 80×24 the two editors stack; wider writing areas put them side by side.

Only one authoritative editor may own a canonical file. Choosing a document already open focuses its existing pane, including symlink/hard-link aliases on Linux. Both buffers use the existing autosave/history/conflict/recovery path and raw YAML frontmatter. Strict focus hides the secondary; leave with Ctrl-D before using F11. A secondary and an old read-only reference are retained but not drawn simultaneously. No file is copied merely to open another view.

## Sticky-note pins

Choose Pinboard… or Pin this document in Ctrl-K or Tools › Notes. Cards reference existing Markdown sources, keep independent scroll and heading/quote anchors, and never become a competing text store. Enter explicitly edits a source through the existing owner-aware editor. Scopes, category/color/size/order, recoverable card archive/Trash and narrow-screen fallback are explained in the Sticky-note pins help topic.

Writing journal, Scratchpad and Session handoff open from Tools › Notes or Ctrl-K.
Writing journal keeps saved project facts and original Markdown
reflections. Scratchpad lets you link original notes to scenes and promote them
to scenes. Session handoff shows the intention you wrote for the next session.

## Unified WrOS settings

Open Settings from the menu bar. Its five groups are Writing, Look and keyboard,
System, Backups and accounts, and Updates and recovery. Settings opens over the
writing page and uses arrow keys, Enter and Esc. System contains Wi-Fi and
networking, Display, Battery and power, Sound and volume, Date and time, and
Storage. Backups, Google Drive, updates and recovery have working controls.
Unsupported services report unavailable. In Display, Warm screen · blue light
filter offers a 15-second Keep/Cancel preview on supported drivers. Battery and
power offers Lock and suspend when available.

## System settings

System settings uses a background worker and cached status. NetworkManager controls Wi-Fi radio and scanning; brightnessctl controls supported backlights. Power controls require confirmation and successful manuscript saving. Unsupported services report unavailable, never success. The Wi-Fi page joins networks; `nmtui`
stays on the maintenance console for emergencies.

The keyboard's brightness and volume keys work everywhere: at the login screen,
while writing and on the lock screen. The screen never goes fully dark, mute
switches sound off and on, and the bottom bar shows the new value within half a
second.

Open Settings › System › Wi-Fi and networking › Scan Wi-Fi networks.
It shows a native list with signal/security and the active network.
Saved Wi-Fi profiles shows the profiles visible to this session;
UUIDs distinguish identical names. Refresh clears old results before retrying,
so an unavailable service cannot leave a stale list looking current. These are
interactive lists: select a scanned network to connect or enter credentials, or
select a saved profile to connect, reconnect or forget it.

Accounts → Local accounts lists supported local login accounts and identifies
this session. Google, Drive, synchronization and backups state their actual
availability. Opening those screens does not enroll an account, fetch documents
or create a verified backup. Local manuscript history is not a separate backup.

Open Settings › System › Storage › Mounted filesystems. It shows mount paths
and capacity. Open Settings › Updates and recovery › Updates.
Choose **Check for WrOS updates** for signed WrOS updates, or **Review a signed
update bundle by path** for an offline NixOS update bundle. If the download stops
partway, choose **Check for WrOS updates** again to carry on where it stopped.
A confirmed update is staged for a restart. **Go back to** `<version>` returns to
the previous system generation when available. The repair controls under **Recovery status,
repair and staged update** lead to **Repair now from retained verified packages**.
On NixOS, this re-activates the current generation. The cached APT package-plan
preview is not available on NixOS.

Keyboard layout selects QWERTY, Colemak or Colemak-DH ANSI/ISO on appliance tty1. A root-owned helper previews for 15 seconds with an independent systemd rollback timer. Type in the preview field; Enter keeps it, Escape reverts. The test field never changes a manuscript. Desktop and SSH sessions must use their own OS layout configuration.

Low-power mode reduces redraws and disables the visualizer; requesting an OS power profile can fail independently.

Open Settings › System › Date and time. Choose timezone loads the installed local
timezone list. Type a city or region, select a result and press Enter. Try Enter
on the default Cancel choice first. To change the timezone, reopen the result,
select Use and confirm. WrOS reads the time service back before reporting success.
An unavailable or denied service leaves writing available and keeps the choice
for retry. Changing timezone affects future local dates; historical records
keep their recorded dates. Set the clock by hand (turns network time off) sets
the clock manually. Use network time again (sets the clock automatically)
restores automatic time.

Battery and power also offers Restart the writing app. It saves, then starts
the writing app again in place; you stay signed in and nothing else restarts.
It waits while a system request or backup is still running.

Battery and power offers Lock writing session on an installed local console
with the locker configured. It saves both documents first; resolve source
conflicts or unavailable storage before retrying. Linux then opens a separate
protected console for your local account password. After successful unlock,
the same writing context returns. If the locker cannot start or fails, the
writing session closes through its save/recovery path and requires a new login.
The passwordless VM demo and desktop/SSH sessions do not offer this action.
This manual lock does not enable sleep or lid/idle locking.

## Interactive tutorial

Choose **Help › Interactive tutorial…** to see all 50 lessons. Press `d` to open the safe demo project: the first time it is made beside your project (Tutorial-1), and after that the same one opens again. Its name shows at the top of the tutorial page, and lessons you have already done carry into it. Your project saves first and is not overwritten. To remove a demo project you no longer want, use Open another project… and press `x` on it. Enter opens a lesson, then launches the actual feature. Progress is verified by application events; a Next button cannot fake completion. Writing exercises only complete in the demo project. `s` skips a lesson; any implemented lesson can be replayed. Help is searchable offline with F1.

## Safety and maintenance

The writing application runs as the writer. Ctrl+Alt+F2 opens the authenticated maintenance console. The NixOS installer creates an encrypted installation and a writer account; root stays locked. Screen locking, guarded suspend, signed generation updates, rollback and phone linking are implemented. VM tests establish their tested behavior, not physical suspend/resume or general hardware qualification.

## The top and bottom bars

The W r O S tiles switch between Write, read, Outline and Study. Click a tile,
or press Super+1 through Super+4. From the menus, Left from File reaches the
tiles. The ⏻ Power tile at the right holds Lock, Sleep,
Restart… (Restart the computer in Ctrl-K) and Shut down…. Esc then W opens it. The power rows ask
first, as they do in Settings › System › Battery and power. On a live USB it
also offers Install WrOS to disk…. About WrOS is the last item in Help. Open
File › Back up now to back up the open project to its configured destinations.

The upper bottom bar shows information about the current view. In Write, with
the outline showing, it shows scene details or the selected word count. With
the outline hidden, it shows the project, chapter, scene and scene words. Click
Wi-Fi, sound, battery or the clock to open a panel. They show networks; volume
and the playing track; charge and brightness; or the date and a month calendar.
The battery panel opens the night light preview. Each panel links to Settings.
Esc or a click outside closes it. Find anything opens them by name too: Wi-Fi, Sound,
Battery and brightness, and Date and calendar.
The lower bar shows project progress, today's words and messages. When no
message is fresh, the playing track scrolls there. Focus mode shows only the page;
a line appears at the bottom only for a failed save, a conflict or a low-battery sleep.
