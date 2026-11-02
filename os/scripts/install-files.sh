#!/bin/bash
# Stage appliance integration into a Debian root. Live-host installation is
# deliberately NOT implemented by this script; never add an implicit '/' path.
#
# The installed-system WrOS payload is the Debian package 'wros' (packaging/manifest.json
# is the single file list, shared with tools/build-wros-debs.py and the image audit).
# This script builds that package from the repository (or takes a prebuilt one in
# WROS_PACKAGE_DEB), installs it with dpkg so /var/lib/dpkg/status records it, and then
# adds only what must not be package-owned: the update trust anchors, unit enablement
# links, masks and the image profile marker.
#
# Build inputs (environment):
#   WROS_UPDATE_KEYRING   binary OpenPGP keyring of the update signer(s)   (required)
#   WROS_UPDATE_SIGNERS   pin file, one 40-hex primary fingerprint per line (required)
#   WROS_PACKAGE_DEB      prebuilt wros_<os/VERSION>_amd64.deb              (optional)
#   WROS_DISPLAY_HW_BINARY prebuilt wros-display-hw                         (optional)
#   SOURCE_DATE_EPOCH, WROS_SOURCE_COMMIT                                   (optional; derived from git)
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
packaging="$repo/packaging/wros_packaging.py"
# Fail before touching the root: unset/empty/secret/mismatched/test update trust inputs.
python3 "$packaging" check-trust
version=$(python3 "$packaging" version)
admindir="$dest/var/lib/dpkg"
[[ -f $admindir/status ]] || { echo 'Staged root has no dpkg database.' >&2; exit 2; }
installed=$(dpkg-query --admindir="$admindir" -W -f='${Version}' wros 2>/dev/null || true)
if [[ -n $installed ]] && dpkg --compare-versions "$installed" gt "$version"; then
    echo "Staged root already has a newer wros ($installed) than this tree ($version)." >&2; exit 2
fi
# Reassembly/upgrades must not reset a committed layout (keyboard.json is a conffile; the
# first-boot adoption marker belongs only to a root that did not have one).
fresh_layout=1
if [[ -e $dest/etc/writingdeck/keyboard.json || -L $dest/etc/writingdeck/keyboard.json ]]; then fresh_layout=0; fi
if [[ -e $dest/etc/pam.d/vlock || -L $dest/etc/pam.d/vlock ]]; then
    if [[ -L $dest/etc/pam.d/vlock ]] || ! cmp -s "$repo/os/config/vlock.pam" "$dest/etc/pam.d/vlock"; then
        echo 'Existing locker PAM configuration preserved; review it before staging this profile.' >&2; exit 2;
    fi
fi
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
if [[ -n ${WROS_PACKAGE_DEB:-} ]]; then
    deb=$WROS_PACKAGE_DEB
    [[ -f $deb && ! -L $deb ]] || { echo 'WROS_PACKAGE_DEB is not a regular file.' >&2; exit 2; }
else
    mkdir "$work/bin" "$work/out"
    install -m 755 "$binary" "$work/bin/grimoire"
    args=(--binaries-dir "$work/bin" --version "$version" --output "$work/out")
    if [[ -n ${WROS_DISPLAY_HW_BINARY:-} ]]; then args+=(--display-helper "$WROS_DISPLAY_HW_BINARY"); fi
    python3 "$repo/tools/build-wros-debs.py" "${args[@]}" > "$work/build.json"
    deb="$work/out/wros_${version}_amd64.deb"
fi
[[ $(dpkg-deb --field "$deb" Package) == wros && $(dpkg-deb --field "$deb" Version) == "$version" \
   && $(dpkg-deb --field "$deb" Architecture) == amd64 ]] || {
    echo "Package is not wros $version amd64; refusing to stage it." >&2; exit 2; }
# --force-confold keeps an existing writer layout; --no-triggers leaves unrelated pending
# triggers of the base system exactly as they were.
mkdir -p "$dest/var/log"
dpkg --root="$dest" --log="$dest/var/log/dpkg.log" --force-confold --force-unsafe-io --no-triggers --install "$deb"
[[ $(dpkg-query --admindir="$admindir" -W -f='${Version}\t${Status}' wros) == "$version"$'\t''install ok installed' ]] || {
    echo 'wros is not recorded as installed after dpkg.' >&2; exit 1; }
runtime=${WRITINGDECK_PROSE_RUNTIME:-$repo/build/prose-runtime/runtime}
python3 "$repo/tools/stage-prose-runtime.py" "$dest" "$runtime"
printf 'IMAGE_PROFILE=%s\n' "$profile" > "$dest/etc/writingdeck/image-profile"
chmod 644 "$dest/etc/writingdeck/image-profile"
if [[ $fresh_layout == 1 && $profile == installer ]]; then
    # Debian Installer populates /etc/default/keyboard on the target later.
    # Adopt that choice at first boot, never on an existing installation.
    install -m 644 /dev/null "$dest/etc/writingdeck/installer-keyboard-pending"
fi
# Debian's online sources: a template only. The installer's target hygiene copies it to
# /etc/apt/sources.list.d after the offline install, never onto the live medium itself.
install -D -m 644 "$repo/os/config/apt/debian.sources" "$dest/usr/share/wros/apt/debian.sources"
if [[ ! -e $dest/etc/pam.d/vlock && ! -L $dest/etc/pam.d/vlock ]]; then
    install -m 644 "$repo/os/config/vlock.pam" "$dest/etc/pam.d/vlock"
fi
# Trust anchors are image inputs, never package-owned: an update must not be able to replace
# the keys that authenticate updates. The package's own /usr/share/wros/update-keyring.gpg
# may only add subkeys of these pinned primaries (the engine and the audit refuse others).
python3 "$packaging" install-trust "$dest"
# Enable units only when the installed unit file itself declares the target.
python3 "$packaging" enable-units "$dest"
# Manual locking does not qualify sleep/resume. Block automatic and explicit
# sleep in the image as well as in the UI; logind's default lid action bypasses it.
for unit in sleep suspend hibernate hybrid-sleep suspend-then-hibernate; do
    ln -sf /dev/null "$dest/etc/systemd/system/$unit.target"
done
for unit in getty@tty8.service autovt@tty8.service; do
    ln -sf /dev/null "$dest/etc/systemd/system/$unit"
done
