# rc30 interface layout: tiles, menus, bottom bars and the new names

Status: **agreed with the owner 2026-11-25. Mock-ups come first, at real size, before any code.**
Part of rc30's visual pass ([Brain design](2026-11-22-brain-library-review-design.md), roadmap item "visual pass").

## Why

On rc29, the `Write │ Library │ Brain │ Review` view bar showed up only in the three new views.
Nobody had decided that. It was left over from rc25, when Brain was the only new view.
Adding the view bar to Write as well would have made two bars at the top, which the owner found clunky.
The owner also doesn't want menus or extra lines crowding the page.
This design puts the view switcher into the WrOS logo itself and gives every piece of status a fixed place.

## The four views get the logo's letters

| Tile | View | Was | Super |
|---|---|---|---|
| **W** | Write | Write | Super+1 |
| **r** | read | Library | Super+2 |
| **O** | Outline | Brain | Super+3 |
| **S** | Study | Review | Super+4 |

"read" is lower case, as the r is in the logo. The keys follow the logo's order. That's already rc29's order (Write, Library, Brain, Review), so only the names change and no key moves.

## Screen layout

```
[W][r][O][S] │ File  Edit  View  Project  Tools  Settings  Help            ✓  [⏻]   ← top bar
┌──────────────────────────────────────────────────────────────────────────────┐
│ …the page…                                                                   │
└──────────────────────────────────────────────────────────────────────────────┘
 info section                                         ▂▄▆  🔈 60%  81%  14:02   ← upper bottom bar
 42,300 / 80,000 ━━━━━━━━━━━━━━──────────  today 412          messages / ♪ now playing   ← lower bottom bar
```

There are three lines of chrome, against two in rc29, so Write gives up one line of text.

### Top bar (one line)

- **The unwrapped logo:** four boxed tiles, **W r O S**, at the far left.
  - The current view's tile is filled with the theme's accent. Its letter takes the background colour.
  - The other tiles have box lines in the border colour and dim letters.
  - **Clicking a tile switches to that view.** Super+1–4 does the same.
  - From the keyboard, F10 opens the menus. Pressing ← from File steps onto the tiles, and Enter switches.
  - Find anything (a Super tap) lists the four views too.
  - When cards are due, **a dot in the warning colour sits in the S tile's corner**. The count shows in Study's own info section.
  - Each tile is two hand-drawn glyphs in `assets/WrOSMono.ttf`: a box with its letter, 16×16 px. They work in the native session and in Foot. Whether the tiles get a light bevel is decided in the mock-ups.
- **A thin divider, then the menus:** File, Edit, View, Project, Tools, Settings, Help.
  - There's no WrOS menu any more. The tiles took its place.
  - **The four views come out of the View menu.** The tiles are now how you switch.
  - **About WrOS** moves to the end of Help.
- **The save, backup and conflict marks** (✓ ○ ◌ ● ⚠) stay at the right, with the same symbols, colours and priorities.
- **The ⏻ tile** sits at the far right and opens its own menu: Lock, Sleep, Restart…, Shut down…, and in the live session Install to disk…. Quit, in builds that offer it, goes here too.

### Menus and popups: the Platinum look

The menus still drop down, but they're drawn as lifted off the page, the way Mac OS 9's Platinum menus were:

- A dropdown uses the header's band colour and hangs straight from its title, with no border line between them.
- The selected item gets a solid accent bar, and thin separators split the items into groups.
- **A one-cell shadow** sits along the right and bottom edges. It darkens the cells underneath but doesn't clear them, so the text stays readable through it. Turbo Vision used this text-mode technique.
- The widget popups and the ⏻ menu use the same style.

### Upper bottom bar

- **Left: the info section.** **Rule (the owner's): it never shows anything already easy to read somewhere else on screen.**
  - **Write, outline showing:** the tree already shows the project's name, the open scene and every word count. So this shows what the tree doesn't: the scene's status and goal, or "edited 14 min ago", taken from Scene details. While text is selected, it shows the selection's word count.
  - **Write, outline hidden or screen too narrow:** project › chapter › scene · scene words. Long paths are shortened from the left, so the scene name always stays.
  - **read:** on the shelf, how many works there are, the filter and the sort order. With a book open, its title, the page and how far through it you are.
  - **Outline:** the path from Today down to the current page, and how many notes link to it.
  - **Study:** the filter, card N of M, and how many cards are left today.
  - Whatever a view used to show in its own title, header or controls line moves here, and that line goes. read, Outline and Study keep their height.
- **Right: the widgets.** Wi-Fi, sound, battery and the clock. **Each one opens a popup panel above itself**, in the Platinum style. Esc, or a click anywhere else, closes it. Each panel ends with a link to its full Settings page.
  - **Wi-Fi:** nearby networks with their signal strength, Wi-Fi on or off, airplane mode.
  - **Sound:** volume, mute, the output device, and the track that's playing with play, pause and skip.
  - **Battery:** charge and time left, a **brightness slider**, a **night light toggle**, and the low-battery and power-saving settings. A better night light is tracked in #173.
  - **Clock:** the date and a month calendar. WrOS has no calendar yet, so this part is new.
  - Find anything can open each panel by name, for example "Wi-Fi".

### Lower bottom bar

- **Left: the project's progress.** The total over the target, with a **longer progress bar** than rc29's ten segments, which uses the free width. Then today's words, or "rest day", and a sprint when one is running.
- **Right: messages.** Exports, errors, battery warnings, and descriptions of menu items while a menu is open.
  - Battery warnings take precedence over everything else.
  - **When no message is fresh, the track that's playing scrolls here.** It scrolls slowly and pauses at each end, and any new message takes over the spot straight away. With Motion turned off, it stays still and shortens the title with "…".

### Focus mode (my call, to confirm on the mock-ups)

- The top bar hides, as it does today.
- The bottom shrinks to one line: the info section, today's words and any warnings. That's close to rc29's focus line.

### Themes and the console

- Everything takes its colours from the theme's roles: accent, border, dim, warning and the band. It changes as soon as the theme does, including with Custom themes.
- **Rainbow:** the current tile is drawn in the moving spectrum, like its other accent-coloured parts.
- **The plain Linux console** can't load the extra glyphs:
  - The tiles fall back to `[W][r][O][S]`, with the current one in reverse colour.
  - The ⏻ tile falls back to `[Power]`.
  - The menus keep the shadow, using the console's 16 colours.

## Renames (the owner is fine with breaking earlier release candidates)

There are no notes or works in the store yet, so nothing needs migrating. As a courtesy, if an old `~/Documents/Brain` exists, it gets moved once.

| What | rc29 | rc30 |
|---|---|---|
| The notes store, shared by read, Outline and Study | `~/Documents/Brain` | `~/Documents/Commonplace` |
| The cache | `~/.cache/grimoire/brain` | `~/.cache/grimoire/commonplace` |
| Code modules | `src/app/brain.rs`, `library/`, `review.rs` | `outline.rs`, `read/`, `study.rs` |
| The view type | `DeskView::{Library, Brain, Review}` | `DeskView::{Read, Outline, Study}` |
| The project setting | `brain_tag` | `outline_tag` |
| The command | `wros brain` | `wros outline` |
| Restored copies | `Brain-restored-…` | `Commonplace-restored-…` |
| Help, menus, Find anything, messages, tests | Brain, Library, Review | Outline, read, Study |

The `grimoire-brain` crate is the store behind all three views. Renaming it to `grimoire-commonplace` is optional, and part of the same sweep if done.

## Ideas tried and turned down (2026-11-25), so they aren't proposed again

- The view bar on Write as well, as a second line at the top.
- The menus' items on the second line in place of the titles, or titles moved to the second line.
- A Lotus-style line of commands, a side column that reflows the text, or a header that pushes the text down.
- NeXTSTEP vertical menus that can be torn off. The owner likes the idea, but it's too hard in ratatui.
- A two-line header in its various forms:
  - tall letters
  - pinstripes
  - titles centred between the two lines
  - a 2×2 logo at the top right, or at the top left as a menu
  - an information line above the menus

## Mock-ups to make first

1. The whole Write screen at 1366×768 (Unifont 8×16, 170×48 cells), with the outline showing and hidden.
2. read, Outline and Study, each with its info section.
3. File open, the ⏻ menu, and each of the four widget popups.
4. Each of those in the Grimoire, Gruvbox Dark, Nord and Rainbow themes, plus the plain console's fallback.
5. Focus mode.

## Out of scope

- Night light improvements (#173).
- The full calendar (only the clock popup's month view is in scope).
- How Brain, Library and Review look inside their pages. That's the rest of the rc30 visual pass, and it builds on these names.
