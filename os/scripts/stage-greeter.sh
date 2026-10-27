#!/bin/bash
# Explicit experimental opt-in, only into an existing Debian staging root.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
dest=$(realpath "${1:?usage: stage-greeter.sh STAGING_ROOT GREETER_BINARY}")
binary=${2:?usage: stage-greeter.sh STAGING_ROOT GREETER_BINARY}
[[ $EUID == 0 && $dest != / && -f $dest/etc/debian_version && -x $binary ]] || {
    echo 'Requires root, an existing staged Debian root (not /), and built greeter.' >&2; exit 2;
}
# Require the maintained Debian package and its locked service account/PAM stack.
[[ -x $dest/usr/sbin/greetd && -f $dest/etc/pam.d/greetd && -x $dest/usr/libexec/writingdeck-session ]] || {
    echo 'Stage the Debian greetd package and WrOS session first.' >&2; exit 2;
}
chroot "$dest" getent passwd _greetd >/dev/null
mkdir -p "$dest/etc/greetd" "$dest/etc/systemd/system" "$dest/usr/bin"
systemctl --root="$dest" disable greetd.service
install -m 755 "$binary" "$dest/usr/bin/wros-greeter"
install -m 644 "$repo/os/config/greetd.toml" "$dest/etc/greetd/config.toml"
install -m 644 "$repo/os/config/greetd.service" "$dest/etc/systemd/system/greetd.service"
# Only tty1 belongs to greetd. Keep tty2 and later authenticated gettys for recovery.
systemctl --root="$dest" disable getty@tty1.service
systemctl --root="$dest" mask getty@tty1.service
systemctl --root="$dest" enable greetd.service
