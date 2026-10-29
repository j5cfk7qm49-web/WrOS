#!/bin/bash
# Build regular files as a normal user. Debian owns all installer engines.
# Provision authenticated APT inputs first. Build dependencies: the normal
# Debian installer dependencies under profile wros-initrd, plus fakeroot,
# initramfs-tools-core, file, build-essential, dpkg-dev, patch and Python 3.
set -euo pipefail
[[ $# == 4 && $EUID != 0 ]] || {
    echo 'Run non-root: build-live-backend.sh DEBIAN_INSTALLER_DSC KERNEL_DEB FRONTEND_BUILD NEW_OUTPUT_DIR' >&2; exit 2;
}
repo=$(cd "$(dirname "$0")/../.." && pwd)
dsc=$(realpath "$1")
archive="$(dirname "$dsc")/debian-installer_20250803+deb13u7.tar.xz"
kernel=$(realpath "$2")
frontend=$(realpath "$3")
dest=$(realpath -m "$4")
[[ ! -e $dest && ! -L $dest && $dest =~ ^/[a-zA-Z0-9/_.-]+$ ]] || {
    echo 'Use a new absolute Linux build directory without spaces or metacharacters.' >&2; exit 2;
}
printf '%s  %s\n' \
    2542e25aff5b98f318d2cf9c642c384e254141ad98b6871e2048e1a5e9a2ab2e "$dsc" \
    3703614db280645e10ce4efd6e255081495c48f2acd66bbdbfca571b4ae860b6 "$archive" \
    a4ad0bf9f084b199dd2b12cb8a2ed918abb442da467f40b772166a267c784a5b "$kernel" | sha256sum -c -
[[ $(dpkg --print-architecture) == amd64 ]] || { echo 'Only Debian amd64 is qualified.' >&2; exit 2; }
python3 - "$repo" "$frontend" <<'PY'
from pathlib import Path
import hashlib,json,sys
repo,frontend=map(Path,sys.argv[1:])
record=json.loads((frontend/'manifest.json').read_bytes())
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert record['architecture']=='amd64' and record['version']=='0.280'
assert record['frontend_debug_formats_absent']
assert record['module_sha256']==digest(frontend/'passthrough.so')
assert record['patch_sha256']==digest(repo/'os/config/installer/cdebconf-passthrough-framing.patch')
assert (frontend/'source/src/modules/frontend/passthrough/passthrough.c').is_file()
PY
mkdir "$dest"
dpkg-source -x "$dsc" "$dest/source"
dpkg-deb -x "$kernel" "$dest/kernel"
cd "$dest/source"
patch --batch --forward --fuzz=0 -p1 < "$repo/os/config/installer/debian-installer-initrd-profile.patch"
mkdir -p "$dest/licenses"
cp debian/copyright "$dest/licenses/debian-installer-copyright"
cp "$frontend/COPYING.passthrough" "$dest/licenses/"
cp "$frontend/copyright" "$dest/licenses/cdebconf-copyright"
cp -a "$frontend/source" "$dest/frontend-source"

# A separate udeb adds the module; the stock text rescue frontend stays available.
package="$dest/passthrough-package"
mkdir -p "$package/DEBIAN" "$package/usr/lib/cdebconf/frontend" build/localudebs
cat > "$package/DEBIAN/control" <<'CONTROL'
Package: wros-cdebconf-passthrough
Version: 0.280+wros2
Architecture: amd64
Section: debian-installer
Priority: optional
Maintainer: WrOS project
Depends: cdebconf-udeb (= 0.280), libc6-udeb (>= 2.41)
Description: WrOS framed passthrough transport for the maintained Debian installer
CONTROL
install -m644 "$frontend/passthrough.so" "$package/usr/lib/cdebconf/frontend/"
dpkg-deb --root-owner-group -Zxz --build "$package" build/localudebs/wros-cdebconf-passthrough.udeb
cd build
cat > config/local <<CONFIG
LINUX_KERNEL_ABI = 6.12.113+deb13
DRM_DIR = $dest/kernel/usr/lib/modules/6.12.113+deb13-amd64/kernel/drivers/gpu/drm
DRIVERS_DIR = $dest/kernel/usr/lib/modules/6.12.113+deb13-amd64/kernel/drivers
EXTRAS += wros-cdebconf-passthrough apt-cdrom-setup
CONFIG
keyring=/usr/share/keyrings/debian-archive-keyring.pgp
[[ -f $keyring ]] || keyring=/usr/share/keyrings/debian-archive-keyring.gpg
[[ -f $keyring ]] || { echo 'Debian archive keyring is required.' >&2; exit 2; }
cat > sources.list.udeb.local <<APT
deb [trusted=yes] copy:$dest/source/build/ localudebs/
deb [signed-by=$keyring] https://deb.debian.org/debian trixie main/debian-installer
deb [signed-by=$keyring] https://security.debian.org/debian-security trixie-security main/debian-installer
APT
epoch=$(dpkg-parsechangelog -l ../debian/changelog -S Timestamp)
DEB_BUILD_PROFILES=wros-initrd SOURCE_DATE_EPOCH="$epoch" \
    fakeroot make build_cdrom_isolinux TARGET='dest/cdrom/initrd.gz dest/cdrom/vmlinuz' EXTRATARGETS= DEPTHCHARGE=
install -m644 dest/cdrom/initrd.gz dest/cdrom/vmlinuz "$dest/"
python3 - "$repo" "$dest" "$frontend" "$dsc" "$archive" "$kernel" <<'PY'
from pathlib import Path
import gzip,hashlib,json,subprocess,sys
repo,dest,frontend,dsc,archive,kernel=map(Path,sys.argv[1:])
sys.path.insert(0,str(repo/'tools'))
from writingdeck_source import fingerprint
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
build=dest/'source/build'
tree=build/'tmp/cdrom_isolinux/tree'
version='6.12.113+deb13-amd64'
assert sorted(p.name for p in (tree/'usr/lib/modules').iterdir())==[version]
assert digest(dest/'vmlinuz')==digest(dest/('kernel/boot/vmlinuz-'+version))
expected=digest(frontend/'passthrough.so')
udebs={p.relative_to(build).as_posix():digest(p)
       for directory in ('apt.udeb/cache/archives','udebs')
       for p in sorted((build/directory).glob('*.udeb'))}
assert len(udebs)>=90 and any(Path(n).name.startswith('wros-cdebconf-passthrough') for n in udebs)
assert any(Path(n).name.startswith('apt-cdrom-setup_') for n in udebs)
assert digest(tree/'usr/lib/cdebconf/frontend/passthrough.so')==expected
image=gzip.decompress((dest/'initrd.gz').read_bytes())
module=subprocess.check_output(['cpio','-i','--to-stdout','--quiet','usr/lib/cdebconf/frontend/passthrough.so'],input=image)
assert hashlib.sha256(module).hexdigest()==expected
cdrom_generator=tree/'usr/lib/apt-setup/generators/40cdrom'
assert cdrom_generator.is_file() and cdrom_generator.stat().st_mode & 0o111
packed_generator=subprocess.check_output(['cpio','-i','--to-stdout','--quiet',
    'usr/lib/apt-setup/generators/40cdrom'],input=image)
assert hashlib.sha256(packed_generator).hexdigest()==digest(cdrom_generator)
manifest={'passed':True,'upstream':'Debian Installer','version':'20250803+deb13u7',
          'architecture':'amd64','kernel_version':version,'kernel_bytes_match_signed_debian_package':True,
          'source_descriptor_sha256':digest(dsc),'source_archive_sha256':digest(archive),
          'kernel_archive_sha256':digest(kernel),
          'initrd_sha256':digest(dest/'initrd.gz'),'kernel_sha256':digest(dest/'vmlinuz'),
          'module_sha256':expected,'module_observed_in_compressed_initrd':True,
          'offline_cdrom_apt_provider':'apt-cdrom-setup',
          'offline_cdrom_apt_generator_sha256':digest(cdrom_generator),
          'offline_cdrom_apt_generator_observed_in_compressed_initrd':True,
          'profile_patch_sha256':digest(repo/'os/config/installer/debian-installer-initrd-profile.patch'),
          'frontend_build_manifest_sha256':digest(frontend/'manifest.json'),
          'recipe_sha256':digest(repo/'os/scripts/build-live-backend.sh'),
          'source_fingerprint':fingerprint(),'udeb_sha256':udebs,
          'source_trust':'Source and kernel pins from authenticated Debian APT; maintained builder verifies archive udeb indexes. Custom frontend udeb is locally built from retained patched source, not Debian-signed.',
          'scope':'Non-root regular-file initrd/kernel build only; no privileged launcher, complete live ISO, installation, reboot, Secure Boot or hardware qualification.'}
(dest/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({k:manifest[k] for k in ('passed','kernel_version','initrd_sha256','module_observed_in_compressed_initrd')}))
PY
