#!/bin/sh
# Translate maintained XKB maps using Debian's console-setup compiler, not app
# character remapping. Includes Unicode, modifiers and function-key strings.
set -eu
output=${1:?usage: generate-keymaps.sh OUTPUT_DIRECTORY}
mkdir -p "$output"

# The console sends a bare arrow whatever modifier is held, so Shift and Ctrl
# arrows (selecting, moving a word at a time) never reached the app. Give them
# the xterm sequences the app reads, in function-key slots ckbcomp leaves free
# (it uses F1-F48). Alt+Left/Right keep switching consoles.
modified_arrows() {  # sets held, code, keycode, final, sequence, slot
    slot=100
    for modifier in shift:2 control:5 'shift control:6'; do
        held=${modifier%%:*}
        code=${modifier#*:}
        for key in 103:A 108:B 106:C 105:D 102:H 107:F 104:5 109:6; do
            keycode=${key%%:*}
            final=${key#*:}
            case $final in
                [0-9]) sequence="\033[$final;$code~" ;;
                *) sequence="\033[1;$code$final" ;;
            esac
            printf '%s keycode %s = F%s\nstring F%s = "%s"\n' "$held" "$keycode" "$slot" "$slot" "$sequence"
            slot=$((slot + 1))
        done
    done
}
for item in 'qwerty:' 'colemak:colemak' 'colemak-dh-ansi:colemak_dh' 'colemak-dh-iso:colemak_dh_iso'; do
    name=${item%%:*}
    variant=${item#*:}
    if [ -n "$variant" ]; then
        ckbcomp -model pc105 -layout us -variant "$variant" > "$output/$name.map"
    else
        ckbcomp -model pc105 -layout us > "$output/$name.map"
    fi
    modified_arrows >> "$output/$name.map"
    # Binary compilation parses the map without opening a real console.
    # Unlike loadkeys --parse, --bkeymap also works in a container/CI.
    loadkeys --unicode --bkeymap "$output/$name.map" > /dev/null
done
