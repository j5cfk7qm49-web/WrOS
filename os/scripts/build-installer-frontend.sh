#!/bin/bash
# Build only the maintained cdebconf passthrough module, never a disk installer.
# Prerequisites: build-essential autoconf automake pkg-config dpkg-dev patch
# libdebian-installer4-dev. Source archives must first come from authenticated APT.
set -euo pipefail
[[ $EUID != 0 && ( $# == 2 || ( $# == 3 && $3 == --test-runtime ) ) ]] || {
    echo 'Run non-root: build-installer-frontend.sh cdebconf_0.280.dsc NEW_OUTPUT_DIR [--test-runtime]' >&2; exit 2;
}
test_runtime=false
[[ $# != 3 ]] || test_runtime=true
repo=$(cd "$(dirname "$0")/../.." && pwd)
dsc=$(realpath "$1")
archive="$(dirname "$dsc")/cdebconf_0.280.tar.xz"
dest=$(realpath -m "$2")
[[ $(basename "$dsc") == cdebconf_0.280.dsc && ! -e $dest && ! -L $dest ]] || {
    echo 'Requires the pinned Debian source descriptor and a new output directory.' >&2; exit 2;
}
printf '%s  %s\n' \
    36cdaa45fca445d2164fbdd81faad9918d748fa703678d8af237354d5744b1d1 "$dsc" \
    c42115131939845e452a323ceded937efad3b2e3e1b80a1a186f9d5c494a43a2 "$archive" | sha256sum --check --status
mkdir -m700 "$dest"
dpkg-source -x "$dsc" "$dest/source"
cd "$dest/source"
patch --batch --forward --fuzz=0 -p1 < "$repo/os/config/installer/cdebconf-passthrough-framing.patch"
autoreconf -fi
export CFLAGS="${CFLAGS:--O2 -fstack-protector-strong -D_FORTIFY_SOURCE=2}"
export LDFLAGS="${LDFLAGS:--Wl,-z,relro,-z,now}"
configuration=/etc/cdebconf.conf
if $test_runtime; then configuration="$dest/test-runtime/etc/cdebconf.conf"; fi
./configure --prefix=/usr --libexecdir=/usr/lib --with-conffile="$configuration" \
    --with-db=rfc822db --with-frontend=passthrough --disable-selinux \
    --enable-d-i --without-debug --without-rpath
if $test_runtime; then make -C src -j2; else make -C src/modules/frontend/passthrough -j2; fi
install -m644 src/modules/frontend/passthrough/passthrough.so "$dest/passthrough.so"
if $test_runtime; then
    runtime="$dest/test-runtime"
    mkdir -p "$runtime/usr/lib/cdebconf/"{db,frontend} "$runtime/etc"
    install -m755 src/debconf "$runtime/usr/lib/cdebconf/debconf"
    install -m644 src/libdebconf.so "$runtime/usr/lib/cdebconf/libdebconf.so"
    install -m644 src/modules/db/rfc822db/rfc822db.so "$runtime/usr/lib/cdebconf/db/"
    install -m644 "$dest/passthrough.so" "$runtime/usr/lib/cdebconf/frontend/"
    install -m600 /dev/null "$configuration"
fi
python3 - "$repo" "$dest" "$dsc" "$archive" "$test_runtime" <<'PY'
from pathlib import Path
import hashlib,json,subprocess,sys,os
repo,dest,dsc,archive=map(Path,sys.argv[1:5])
test_runtime=sys.argv[5]=='true'
module=dest/'source/src/modules/frontend/passthrough/passthrough.c'
# Retain the module's original BSD notice and the full upstream copyright index.
text=module.read_text()
(dest/'COPYING.passthrough').write_text(text[:text.index('*/')+2]+'\n')
(dest/'copyright').write_bytes((dest/'source/debian/copyright').read_bytes())
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
manifest={'upstream':'Debian cdebconf','version':'0.280','architecture':subprocess.check_output(['dpkg','--print-architecture'],text=True).strip(),
          'source_descriptor_sha256':digest(dsc),'source_archive_sha256':digest(archive),
          'patch_sha256':digest(repo/'os/config/installer/cdebconf-passthrough-framing.patch'),
          'module_sha256':digest(dest/'passthrough.so'),
          'compiler':subprocess.check_output(['cc','--version'],text=True).splitlines()[0],
          'cflags':os.environ['CFLAGS'],'ldflags':os.environ['LDFLAGS'],
          'private_test_runtime':test_runtime,
          'frontend_debug_formats_absent':all(value not in (dest/'passthrough.so').read_bytes() for value in (b'----> %s',b'<---- %s',b'Got "%s" for %s')),
          'scope':'Frontend protocol and response handling only; no changes to partitioning, encryption, account or installation engines.',
          'source_trust':'Pinned hashes of archives retrieved through authenticated Debian APT indexes; no independent maintainer-signature claim.'}
(dest/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
PY
echo "Built passthrough.so and source/license manifest in $dest; live-install qualification is still required."
