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
| Ctrl-K | Existing command palette, including new home/system/workspaces/tutorial actions |
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

Available alias actions: dashboard, system-settings, tutorial, outline, secondary, focus, save, find, palette. More complete shortcut customization is planned. F10/F11/F12 remain default fallback actions even if aliases are removed; an explicit alias on one of these keys takes precedence.

## Layout and terminal limits

Shortcuts refer to logical letters after Linux/XKB has translated keys. Text is never remapped again for Colemak. Raw physical-key positions cannot reliably be reconstructed from terminal escape sequences. Use arrows/function keys when layout-independent positions matter. Avoid Ctrl+Alt aliases that a particular locale's AltGr arrangement produces ambiguously. Existing non-ASCII AltGr/Unicode input normalization is retained.

The terminal may not distinguish Ctrl-I from Tab or Ctrl-M from Enter. Existing menu/palette actions remain the discoverable fallback. Generic tty support does not promise every Kitty-protocol or hardware scan-code feature.

## Tutorial and tests

Tutorial lessons 2, 8, 9, 10 and 13 use the actual layout/focus/outline/alias/secondary-edit features. TUI tests exercise translated Unicode and logical aliases labeled across QWERTY/Colemak/DH scenarios. Real console DH mapping is a separate VM assertion; physical keyboard verification remains outstanding. No menu-description/help/key guard test was removed to add these features.
