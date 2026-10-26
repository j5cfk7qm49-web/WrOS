#!/bin/bash
# Stage appliance integration into a Debian root. Live-host installation is
# deliberately NOT implemented by this script; never add an implicit '/' path.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
dest=${1:?usage: install-files.sh STAGING_ROOT GRIMOIRE_BINARY}
binary=${2:?usage: install-files.sh STAGING_ROOT GRIMOIRE_BINARY}
dest=$(realpath "$dest")
[[ $EUID == 0 && $dest != / && -f $dest/etc/debian_version && -x $binary ]] || {
    echo 'Requires root, an existing staged Debian root (not /), and a built binary.' >&2; exit 2;
}
mkdir -p "$dest/usr/bin" "$dest/usr/libexec" "$dest/etc/profile.d" "$dest/etc/writingdeck" \
    "$dest/etc/polkit-1/rules.d" "$dest/usr/share/polkit-1/actions" \
    "$dest/usr/share/doc/writingdeck" "$dest/usr/share/writingdeck/keymaps" "$dest/etc/systemd/system"
install -m 755 "$binary" "$dest/usr/bin/grimoire"
strip "$dest/usr/bin/grimoire"
install -m 755 "$repo/os/scripts/writingdeck-keyboard" "$repo/os/scripts/writingdeck-session" "$dest/usr/libexec/"
install -m 644 "$repo/os/config/writingdeck-profile.sh" "$dest/etc/profile.d/writingdeck.sh"
# Reassembly/upgrades must not reset a committed layout.
if [[ ! -e $dest/etc/writingdeck/keyboard.json ]]; then
    install -m 644 "$repo/os/config/keyboard.json" "$dest/etc/writingdeck/keyboard.json"
fi
install -m 644 "$repo/os/config/50-writingdeck-keyboard.rules" "$dest/etc/polkit-1/rules.d/"
install -m 644 "$repo/os/config/org.writingdeck.keyboard.policy" "$dest/usr/share/polkit-1/actions/"
install -m 644 "$repo/os/systemd/"* "$dest/etc/systemd/system/"
"$repo/os/scripts/generate-keymaps.sh" "$dest/usr/share/writingdeck/keymaps"
install -m 644 "$repo/LICENSE" "$dest/usr/share/doc/writingdeck/LICENSE-grimoire"
install -m 644 "$repo/assets/dict/DICTIONARY-LICENSE" "$dest/usr/share/doc/writingdeck/DICTIONARY-LICENSE"
# Debian package copyright files remain in the image; no upstream notices stripped.
for f in "$repo"/docs/*.md "$repo"/docs/*licenses.json "$repo"/docs/feature-status.json \
    "$repo"/docs/keyboard-reference.html "$repo"/os/licenses/*.txt; do
    [[ ! -f $f ]] || install -m 644 "$f" "$dest/usr/share/doc/writingdeck/"
done
# Retain exact omitted upstream notices as well as the combined notice bundle.
mkdir -p "$dest/usr/share/doc/writingdeck/upstream"
install -m 644 "$repo"/os/licenses/upstream/*.txt "$dest/usr/share/doc/writingdeck/upstream/"
install -m 644 "$repo/docs/upstream-notice-sources.json" "$dest/usr/share/doc/writingdeck/"
printf 'WRITINGDECK_VERSION=0.1.0-dev\nGRIMOIRE_BASE=0.7.5\nBASE_COMMIT=f3ca1e8\nIMAGE_PROFILE=vm-demo\n' > "$dest/etc/writingdeck/release"
mkdir -p "$dest/etc/systemd/system/multi-user.target.wants"
ln -sf ../writingdeck-keyboard.service "$dest/etc/systemd/system/multi-user.target.wants/writingdeck-keyboard.service"
