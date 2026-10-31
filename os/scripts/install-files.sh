#!/bin/bash
# Stage appliance integration into a Debian root. Live-host installation is
# deliberately NOT implemented by this script; never add an implicit '/' path.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
dest=${1:?usage: install-files.sh STAGING_ROOT GRIMOIRE_BINARY}
binary=${2:?usage: install-files.sh STAGING_ROOT GRIMOIRE_BINARY}
profile=${3:-vm-demo}
case "$profile" in
    vm-demo|installer) ;;
    *) echo 'Unknown image profile; expected vm-demo or installer.' >&2; exit 2 ;;
esac
dest=$(realpath "$dest")
[[ $EUID == 0 && $dest != / && -f $dest/etc/debian_version && -x $binary ]] || {
    echo 'Requires root, an existing staged Debian root (not /), and a built binary.' >&2; exit 2;
}
mkdir -p "$dest/usr/bin" "$dest/usr/libexec" "$dest/etc/profile.d" "$dest/etc/writingdeck" \
    "$dest/etc/wros" \
    "$dest/etc/polkit-1/rules.d" "$dest/usr/share/polkit-1/actions" \
    "$dest/usr/share/doc/writingdeck" "$dest/usr/share/writingdeck/keymaps" "$dest/etc/systemd/system"
install -m 755 "$binary" "$dest/usr/bin/grimoire"
strip "$dest/usr/bin/grimoire"
runtime=${WRITINGDECK_PROSE_RUNTIME:-$repo/build/prose-runtime/runtime}
python3 "$repo/tools/stage-prose-runtime.py" "$dest" "$runtime"
install -m 755 "$repo/os/scripts/writingdeck-keyboard" "$repo/os/scripts/writingdeck-session" "$dest/usr/libexec/"
install -m 755 "$repo/os/scripts/wros-display" "$dest/usr/libexec/"
install -m 755 "$repo/os/scripts/wros-lock" "$dest/usr/libexec/"
# Build on the declared Debian builder; never ship the fake test DRM library.
display_build=$(mktemp -d)
trap 'rm -f -- "$display_build/wros-display-hw"; rmdir -- "$display_build"' EXIT
read -r -a drm_cflags < <(pkg-config --cflags libdrm)
read -r -a drm_libs < <(pkg-config --libs libdrm)
cc -std=c11 -O2 -Wall -Wextra -Werror -D_FORTIFY_SOURCE=3 -fstack-protector-strong \
    "${drm_cflags[@]}" "$repo/os/helpers/wros-display-hw.c" \
    -Wl,-z,relro,-z,now -o "$display_build/wros-display-hw" "${drm_libs[@]}"
install -m 755 "$display_build/wros-display-hw" "$dest/usr/libexec/"
strip "$dest/usr/libexec/wros-display-hw"
install -m 644 "$repo/os/config/writingdeck-profile.sh" "$dest/etc/profile.d/writingdeck.sh"
# Reassembly/upgrades must not reset a committed layout.
if [[ ! -e $dest/etc/writingdeck/keyboard.json && ! -L $dest/etc/writingdeck/keyboard.json ]]; then
    install -m 644 "$repo/os/config/keyboard.json" "$dest/etc/writingdeck/keyboard.json"
    if [[ $profile == installer ]]; then
        # Debian Installer populates /etc/default/keyboard on the target later.
        # Adopt that choice at first boot, never on an existing installation.
        install -m 644 /dev/null "$dest/etc/writingdeck/installer-keyboard-pending"
    fi
fi
install -m 644 "$repo/os/config/50-writingdeck-keyboard.rules" "$dest/etc/polkit-1/rules.d/"
install -m 644 "$repo/os/config/org.writingdeck.keyboard.policy" "$dest/usr/share/polkit-1/actions/"
install -m 644 "$repo/os/config/50-wros-display.rules" "$dest/etc/polkit-1/rules.d/"
install -m 644 "$repo/os/config/50-wros-timezone.rules" "$dest/etc/polkit-1/rules.d/"
install -m 644 "$repo/os/config/50-wros-lock.rules" "$dest/etc/polkit-1/rules.d/"
install -m 644 "$repo/os/config/org.wros.lock.policy" "$dest/usr/share/polkit-1/actions/"
if [[ -e $dest/etc/pam.d/vlock || -L $dest/etc/pam.d/vlock ]]; then
    if [[ -L $dest/etc/pam.d/vlock ]] || ! cmp -s "$repo/os/config/vlock.pam" "$dest/etc/pam.d/vlock"; then
        echo 'Existing locker PAM configuration preserved; review it before staging this profile.' >&2; exit 2;
    fi
else
    install -m 644 "$repo/os/config/vlock.pam" "$dest/etc/pam.d/vlock"
fi
install -m 644 "$repo/os/config/org.wros.display.policy" "$dest/usr/share/polkit-1/actions/"
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
# Retain every pinned Python notice and its machine-readable source inventory.
cp -a "$repo/os/licenses/python" "$dest/usr/share/doc/writingdeck/"
install -m 644 "$repo/docs/python-tools-lock.json" "$repo/docs/python-tool-notices.json" \
    "$dest/usr/share/doc/writingdeck/"
printf 'WRITINGDECK_VERSION=0.1.0-dev\nGRIMOIRE_BASE=0.7.5\nBASE_COMMIT=f3ca1e8\nIMAGE_PROFILE=%s\n' "$profile" > "$dest/etc/writingdeck/release"
mkdir -p "$dest/etc/systemd/system/multi-user.target.wants"
ln -sf ../writingdeck-keyboard.service "$dest/etc/systemd/system/multi-user.target.wants/writingdeck-keyboard.service"
ln -sf ../wros-display.service "$dest/etc/systemd/system/multi-user.target.wants/wros-display.service"
# Manual locking does not qualify sleep/resume. Block automatic and explicit
# sleep in the image as well as in the UI; logind's default lid action bypasses it.
mkdir -p "$dest/etc/systemd/logind.conf.d" "$dest/etc/systemd/sleep.conf.d"
install -m 644 "$repo/os/config/60-wros-power-guard.conf" "$dest/etc/systemd/logind.conf.d/"
install -m 644 "$repo/os/config/60-wros-sleep-guard.conf" "$dest/etc/systemd/sleep.conf.d/"
# Debian's NetworkManager may ship a connectivity check that requests a Debian
# URL every few minutes. WrOS makes no unsolicited outbound requests.
mkdir -p "$dest/etc/NetworkManager/conf.d"
install -m 644 "$repo/os/config/60-wros-no-connectivity-check.conf" "$dest/etc/NetworkManager/conf.d/"
for unit in sleep suspend hibernate hybrid-sleep suspend-then-hibernate; do
    ln -sf /dev/null "$dest/etc/systemd/system/$unit.target"
done
for unit in getty@tty8.service autovt@tty8.service; do
    ln -sf /dev/null "$dest/etc/systemd/system/$unit"
done
