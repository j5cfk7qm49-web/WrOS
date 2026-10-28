#!/bin/bash
# Stage only regular image files. Never create a live user in the SquashFS.
set -euo pipefail
[[ $# == 2 && $EUID == 0 ]] || { echo 'Run as root: stage-live-session.sh STAGING_ROOT INSTALLER_BINARY' >&2; exit 2; }
repo=$(cd "$(dirname "$0")/../.." && pwd)
dest=$(realpath "$1")
binary=$(realpath "$2")
[[ $dest != / && -f $dest/etc/debian_version && -x $binary \
   && -x $dest/usr/bin/wros-greeter && -x $dest/usr/bin/live-config \
   && -x $dest/usr/lib/user-setup/user-setup-apply \
   && -f $dest/etc/greetd/config.toml && ! -e $dest/etc/writingdeck/VM-DEMO-ONLY ]] || {
    echo 'Requires an existing authenticated Debian live staging root, not / or a VM-demo root.' >&2; exit 2;
}
python3 - "$dest" <<'PY'
from pathlib import Path
import sys,tomllib
root=Path(sys.argv[1]);cfg=tomllib.loads((root/'etc/greetd/config.toml').read_text())
assert 'initial_session' not in cfg
assert cfg['default_session']=={'command':'/usr/bin/wros-greeter','user':'_greetd'}
assert not any(line.split(':')[0]=='wros-live' for line in (root/'etc/passwd').read_text().splitlines())
PY
mkdir -p "$dest/etc/live/config.conf.d" "$dest/usr/libexec" \
    "$dest/lib/live/config" "$dest/etc/systemd/system/greetd.service.d" \
    "$dest/etc/systemd/system-generators" "$dest/usr/bin"
install -m755 "$repo/os/scripts/wros-live-session" "$dest/usr/libexec/"
install -m755 "$repo/os/config/live/9998-wros-session" "$dest/lib/live/config/"
install -m644 "$repo/os/config/live/live.conf" "$dest/etc/live/config.conf.d/90-wros.conf"
install -m644 "$repo/os/config/live/greetd-live.conf" "$dest/etc/systemd/system/greetd.service.d/90-wros-live.conf"
install -m755 "$binary" "$dest/usr/bin/wros-installer"
mask="$dest/etc/systemd/system-generators/live-config-getty-generator"
if [[ -e $mask || -L $mask ]]; then
    [[ -L $mask && $(readlink "$mask") == /dev/null ]] || { echo 'Existing generator override preserved.' >&2; exit 2; }
else
    ln -s /dev/null "$mask"
fi
systemctl --root="$dest" enable live-config.service
echo 'Live session components staged. Account and initial session must be created only in the live boot overlay.'
