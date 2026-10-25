# Keyboard shortcuts and compatibility

The upstream shortcuts remain functional. `assets/help/24-every-key.md` is the compiled searchable reference and its exact key list is tested bidirectionally.

| Key | Current behavior |
|---|---|
| F1 / ? where not typing | Offline help |
| F2 / F3 | Timer start-pause / reset |
| F4–F7 | Existing music controls (optional, off by default) |
| F8 / F9 | Spelling suggestions / themes |
| **F10** | Toggle left outline column |
| **F11** | Choose/show/hide the independent editable secondary pane; menu/palette chooses a replacement |
| **F12** | WritingDeck home dashboard |
| **Ctrl-D** | Strict focus; same established shortcut, tightened hiding/focus behavior |
| Tab / Shift-Tab | Cycle visible panes; never a hidden outline/secondary/reference |
| Ctrl-K | Existing command palette, including home/system/workspaces/tutorial/Pinboard actions |
| Ctrl-S / Ctrl-Q | Save / quit with upstream safety behavior |
| Esc | Existing close/menu behavior; closes dashboard to resume |

## Custom aliases

System settings → Keyboard shortcuts selects an action, then Enter captures a new alias. Choose Ctrl+Alt+A–Z or F10–F12. Plain letters, legacy Ctrl bindings, Tab, Esc and F1–F9 are protected. Duplicate aliases are replaced explicitly in the capture UI; invalid/reserved keys are rejected. This is an **additive compatibility strategy**, not arbitrary rebinding/removal of all legacy shortcuts.

Aliases persist per machine at `paths::data_dir()/writingdeck/shortcuts.json`, using schema v1. Invalid files are preserved and defaults remain usable; an external edit conflicts rather than being overwritten. The file is also human-editable with a validated structure:

```json
{"version":1,"bindings":[
  {"chord":"F10","action":"outline"},
  {"chord":"F11","action":"secondary"},
  {"chord":"F12","action":"dashboard"},
  {"chord":"Ctrl+Alt+G","action":"system-settings"}
]}
```

Available alias actions: dashboard, system-settings, tutorial, outline, secondary, focus, save, find, palette, pinboard, pins (sidebar toggle), pin-current, journal. More complete shortcut customization is planned. F10/F11/F12 remain default fallback actions even if aliases are removed; an explicit alias on one of these keys takes precedence.

## Card controls

Pinboard (`p` on home or Ctrl-K) uses Up/Down for card/list selection, PgUp/PgDn for independent card scrolling, h/q for heading/selected-quote capture, Home for the unique semantic anchor, and Enter/e for explicit original-source editing. v toggles the responsive sidebar. Category input is text, not prose-edit commands. Card controls never remap ordinary editor letters. Full controls and persistence limits: [PINS](PINS.md) or F1 → Sticky-note pins.

## Journal controls

Dashboard j, Writing tools or Ctrl-K opens Writing journal. Left/Right/t select dates, Tab changes facts/reflection view, scrolling does not move either editor, c records current clean measured counts, Enter/e explicitly edits the original reflection, and r reloads/retries pending facts. Reader paste never executes these letters as commands. F1 links the full [journal help](JOURNAL.md); the printable reference includes these modal keys. The explicit force-quit warning is prefixed and outranks journal metadata/count warnings so it stays visible at 80 columns.

## Layout and terminal limits

Shortcuts refer to logical letters after Linux/XKB has translated keys. Text is never remapped again for Colemak. Raw physical-key positions cannot reliably be reconstructed from terminal escape sequences. Use arrows/function keys when layout-independent positions matter. Avoid Ctrl+Alt aliases that a particular locale's AltGr arrangement produces ambiguously. Existing non-ASCII AltGr/Unicode input normalization is retained.

The terminal may not distinguish Ctrl-I from Tab or Ctrl-M from Enter. Existing menu/palette actions remain the discoverable fallback. Generic tty support does not promise every Kitty-protocol or hardware scan-code feature.

## Tutorial and tests

Tutorial lessons 2, 8, 9, 10, 11, 12, 13 and 14 use the actual layout/focus/outline/alias/secondary-edit features. TUI tests exercise translated Unicode and logical aliases labeled across QWERTY/Colemak/DH scenarios. Real console DH mapping is a separate VM assertion; physical keyboard verification remains outstanding. No menu-description/help/key guard test was removed to add these features.
