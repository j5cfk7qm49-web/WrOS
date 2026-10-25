# Sticky-note pins

A pin is a project-local reference to an existing Markdown document, not another copy of its body. Open **Pinboard** in Writing tools, the dashboard (`p`), or Ctrl-K. **Pin current document** pins the active editor's original file. In Pinboard, `n` chooses a source. Two cards may reference one file and keep separate presentation and scroll positions.

## Read cards, edit sources

`Up` and `Down` select cards. `PgUp` and `PgDn` scroll inside the selected card, not its source editor. A mouse wheel over a sidebar or dashboard card scrolls that card without stealing editor focus. A wheel over Pinboard's list moves the list; over its preview it scrolls the card. Click a sidebar card to focus it, or a dashboard card to select it in Pinboard.

Cards are plain-Markdown previews. They cannot be typed into. `Enter` explicitly opens the original source in the independent editor; `e` requests Primary. An already-owned file or alias focuses its existing editor instead of creating a competing buffer. Existing save, undo, history, conflict and recovery protections apply. A failed save or identity check blocks replacement. Pasted prose in Pinboard is ignored, never treated as card commands.

## Scrolling and semantic anchors

Cards start in **Free** mode: scrolling is relative to the start of the current body. `h` captures the first ATX heading (`## A heading`), or cycles through available headings. This enters **Anchored** mode: scrolling is relative to that heading, even if text is inserted above it. `Home` returns to the unique anchor. `q` captures an exact selected passage from this source's active editor (at most 512 characters). Select that passage before reopening Pinboard.

`f` switches between free and anchored modes, resetting the offset. An anchor is required for anchored mode. Heading labels and quotes are case-sensitive. Missing or duplicate anchors are shown explicitly, not guessed; `f` allows a free preview, `h` or `q` replaces the anchor, and `b` explicitly relinks a missing source. Quoted anchor text is small reference metadata, not a second editable note. Scroll offsets count wrapped display rows; resizing may reflow them. Source cursor, selection and undo remain independent.

## Presentation and scope

**Toggle pins sidebar** is available in Writing tools and Ctrl-K. `v` shows or hides the sidebar; `[` and `]` change its width. Cards are retained when the writing area is too narrow or strict focus hides them. Ctrl-K → Pinboard remains available. With two editable documents, the sidebar only appears if at least 72 writing columns remain. Strict focus never tab-focuses a hidden card.

`space` collapses a card, `+` and `-` change its height, `c` cycles its color, `g` edits its category, and `J` and `K` change its order. These settings and scroll positions persist outside manuscript text.

`s` cycles **Always → Novel → Chapter → Scene → Dashboard → Hidden**. Always means this project, not every book. Chapter and scene scopes capture the current manuscript context in Primary, regardless of which editor is focused. Open a manuscript scene in Primary before capturing those scopes. Hidden cards stay accessible in Pinboard. Dashboard-only and Always cards appear beside the home widgets when space permits and the Dashboard pins widget is visible; overflow and narrow-screen cards remain available in Pinboard.

## Archive, Trash and safe persistence

`a` archives a card; `d` sends only the card to recoverable pin Trash. Neither moves, rewrites or deletes its source. `Tab` cycles Active, Archived and Trashed collections; `u` restores a card to Active. No permanent source deletion is offered by Pinboard.

Metadata lives in `.grimoire/writingdeck/pins.json`, with guarded atomic writes. Invalid, unknown, future-version, externally changed or unwritable configuration is preserved and reported; a failed transaction leaves the previous cards in memory. `R` explicitly reloads metadata. This is not a backup. No automatic reset silently discards damaged metadata.

Known in-app file moves update references. A deleted, unreadable, outside-project or unrecognized source remains an explicit unavailable card; it is never silently replaced by another note. Recreate or restore the original file, or use `b` to choose a new source. Containment and alias checks run on input/reload and the bounded disk-check tick, never during rendering. They do not provide an OS-level file lock or protection from a malicious filesystem race.
