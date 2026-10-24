#!/bin/sh
# Translate maintained XKB maps using Debian's console-setup compiler, not app
# character remapping. Includes Unicode, modifiers and function-key strings.
set -eu
output=${1:?usage: generate-keymaps.sh OUTPUT_DIRECTORY}
mkdir -p "$output"
for item in 'qwerty:' 'colemak:colemak' 'colemak-dh-ansi:colemak_dh' 'colemak-dh-iso:colemak_dh_iso'; do
    name=${item%%:*}
    variant=${item#*:}
    if [ -n "$variant" ]; then
        ckbcomp -model pc105 -layout us -variant "$variant" > "$output/$name.map"
    else
        ckbcomp -model pc105 -layout us > "$output/$name.map"
    fi
    # Binary compilation parses the map without opening a real console.
    # Unlike loadkeys --parse, --bkeymap also works in a container/CI.
    loadkeys --unicode --bkeymap "$output/$name.map" > /dev/null
done
