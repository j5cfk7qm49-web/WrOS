#!/bin/bash
# Ordered, fail-closed driver for the WrOS release-candidate live/installer ISO.
#
# Run as root inside the isolated Debian 13 amd64 builder (see docs/INSTALLATION.md).
# Steps that must not run as root (cargo, prose runtime, Debian source builds,
# the offline pool, the capsule stager, the media audit) run as WROS_BUILD_USER.
#
# Flow (STEPS below is the single source of the order; --plan prints it):
#   binaries           release-profile cargo build: grimoire, wros-greeter, wros-installer
#   prose-runtime      locked offline prose runtime (tools/prepare-prose-tools.py)
#   inputs             pinned Debian source/kernel inputs for the backend (hash-checked by the builders)
#   installer-frontend os/scripts/build-installer-frontend.sh   (cdebconf passthrough module)
#   live-backend       os/scripts/build-live-backend.sh         (d-i kernel/initrd backend)
#   live-image         os/scripts/build-installer.sh --live     (live-build up to 'lb binary')
#   native-pool        tools/build-live-installer-pool.py       (lb binary tree is an archive root)
#   native-capsule     tools/stage-native-installer.py --medium (verified regular-file capsule)
#   remaster           copy capsule to <medium>/native-installer, re-run lb binary_checksums/binary_iso
#   audit              tools/audit-installer-media.py on the real ISO (xorriso, unsquashfs)
#   smoke-boot         OVMF boot of the ISO as CD-ROM and as a USB disk (GRUB loads kernel + initrd)
#   publish            release name, checksum and receipt, only after audit and smoke boot pass
#
# Nothing is published under the release name until the audit passes. Failures leave
# *.partial / *.FAILED.json files in the build directory, never a final-named ISO or a
# success receipt. Finished ISOs are never deleted or overwritten.
#
# Inputs (environment):
#   WROS_UPDATE_KEYRING, WROS_UPDATE_SIGNERS  public update-trust inputs (required; passed through
#                                             to install-files.sh; private keys are refused)
#   SOURCE_DATE_EPOCH                         default: committer time of HEAD
#   WROS_BUILD_USER                           non-root build account (default: builder, else SUDO_USER)
#   WROS_INPUT_CACHE, WROS_PROSE_CACHE, WROS_POOL_CACHE   optional local caches (verified by hash)
#   WROS_CARGO_JOBS (default 2), CARGO_TARGET_DIR, CARGO_HOME, RUSTUP_HOME
#
# Firmware: the RC requires UEFI x86-64 with Secure Boot DISABLED. The image is built with
# live-build's Secure Boot files, but no signed boot chain has been tested; that is not claimed.
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
STEPS=(binaries prose-runtime inputs installer-frontend live-backend live-image native-pool \
       native-capsule remaster audit smoke-boot publish)
FALLBACK_VERSION='0.1.0~rc1'
IMAGE_NAME=wros-live
DEBIAN_MIRROR=https://deb.debian.org/debian
SECURITY_MIRROR=https://security.debian.org/debian-security
INPUT_FILES=(
    "$DEBIAN_MIRROR/pool/main/c/cdebconf/cdebconf_0.280.dsc"
    "$DEBIAN_MIRROR/pool/main/c/cdebconf/cdebconf_0.280.tar.xz"
    "$DEBIAN_MIRROR/pool/main/d/debian-installer/debian-installer_20250803+deb13u7.dsc"
    "$DEBIAN_MIRROR/pool/main/d/debian-installer/debian-installer_20250803+deb13u7.tar.xz"
    "$SECURITY_MIRROR/pool/updates/main/l/linux/linux-image-6.12.113+deb13-amd64_6.12.113-1_amd64.deb"
)

log() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die() { printf 'build-release-iso: %s\n' "$*" >&2; exit "${2:-1}"; }

usage() {
    cat >&2 <<'USAGE'
Usage: build-release-iso.sh [--build-dir DIR] [--output-dir DIR] [--resume [--accept-source-drift]]
                            [--interim [--allow-dirty]] [--plan] [--preflight-only]
Run as root in the Debian 13 builder. --interim builds a clearly named non-RC image that may lack
update-trust files (for builders that predate the dpkg/update packaging); it is never an RC.
USAGE
}

# ---------------------------------------------------------------- preflight

missing_commands=()
missing_notes=()

need_command() {   # command, package providing it
    command -v "$1" >/dev/null 2>&1 || { missing_commands+=("$1"); missing_notes+=("$2"); }
}

preflight() {
    missing_commands=(); missing_notes=()
    need_command cc gcc;                need_command pkg-config pkgconf
    need_command strip binutils;        need_command ckbcomp console-setup
    need_command loadkeys kbd;          need_command systemctl systemd
    need_command useradd passwd;        need_command gpgv gpgv
    need_command xorriso xorriso;       need_command mksquashfs squashfs-tools
    need_command unsquashfs squashfs-tools; need_command lb live-build
    need_command debootstrap debootstrap;   need_command fakeroot fakeroot
    need_command dpkg-deb dpkg;         need_command dpkg-source dpkg-dev
    need_command python3 python3;       need_command cpio cpio
    need_command file file;             need_command zstd zstd
    need_command patch patch;           need_command make make
    need_command autoreconf autoconf;   need_command runuser util-linux
    need_command sha256sum coreutils;   need_command git git
    need_command cargo cargo;           need_command qemu-system-x86_64 qemu-system-x86
    need_command mformat mtools;        need_command rsync rsync
    if command -v pkg-config >/dev/null 2>&1; then
        pkg-config --exists libdrm || { missing_commands+=(libdrm-dev); missing_notes+=(libdrm-dev); }
        pkg-config --exists libinput || { missing_commands+=(libinput-dev); missing_notes+=(libinput-dev); }
        pkg-config --exists libudev || { missing_commands+=(libudev-dev); missing_notes+=(libudev-dev); }
        pkg-config --exists libseat || { missing_commands+=(libseat-dev); missing_notes+=(libseat-dev); }
        pkg-config --exists xkbcommon || { missing_commands+=(libxkbcommon-dev); missing_notes+=(libxkbcommon-dev); }
        pkg-config --exists libdebian-installer || { missing_commands+=(libdebian-installer4-dev); missing_notes+=(libdebian-installer4-dev); }
    fi
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import debian.debfile' 2>/dev/null || { missing_commands+=(python3-debian); missing_notes+=(python3-debian); }
        python3 -c 'import apt_pkg' 2>/dev/null || { missing_commands+=(python3-apt); missing_notes+=(python3-apt); }
    fi
    local keyring=
    for candidate in /usr/share/keyrings/debian-archive-keyring.pgp /usr/share/keyrings/debian-archive-keyring.gpg; do
        [[ -s $candidate ]] && { keyring=$candidate; break; }
    done
    [[ -n $keyring ]] || { missing_commands+=('debian-archive-keyring(.pgp|.gpg)'); missing_notes+=(debian-archive-keyring); }
    [[ -r ${WROS_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd} && -r ${WROS_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd} ]] \
        || { missing_commands+=('OVMF firmware'); missing_notes+=(ovmf); }
    for package in isolinux syslinux-common grub-pc-bin grub-efi-amd64-bin dosfstools; do
        dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q '^install ok installed$' \
            || { missing_commands+=("$package"); missing_notes+=("$package"); }
    done
    if ((${#missing_commands[@]} != 0)); then
        printf 'build-release-iso: preflight failed; missing: %s\n' "${missing_commands[*]}" >&2
        printf 'Debian packages to install: %s\n' "$(printf '%s\n' "${missing_notes[@]}" | sort -u | paste -sd' ')" >&2
        return 2
    fi
    return 0
}

preflight_inputs() {
    [[ $EUID == 0 ]] || die 'must run as root in an isolated Debian 13 builder' 2
    [[ -n ${WROS_UPDATE_KEYRING:-} && -n ${WROS_UPDATE_SIGNERS:-} ]] \
        || die 'WROS_UPDATE_KEYRING and WROS_UPDATE_SIGNERS (public update-trust inputs) are required' 2
    python3 -I "$repo/tools/audit-installer-media.py" --check-update-inputs "$WROS_UPDATE_KEYRING" "$WROS_UPDATE_SIGNERS" \
        >/dev/null || die 'update-trust inputs rejected (see message above)' 2
    build_user=${WROS_BUILD_USER:-}
    if [[ -z $build_user ]]; then
        if id builder >/dev/null 2>&1; then build_user=builder; else build_user=${SUDO_USER:-}; fi
    fi
    if [[ -z $build_user || $build_user == root ]] || ! id "$build_user" >/dev/null 2>&1; then
        die 'set WROS_BUILD_USER to a non-root account for the unprivileged steps' 2
    fi
    build_home=$(getent passwd "$build_user" | cut -d: -f6)
    [[ -d $build_home ]] || die "home directory of $build_user does not exist" 2
}

# Run a command as the unprivileged build user with a clean, explicit environment.
as_builder() {
    local -a keep=()
    local name
    for name in WROS_UPDATE_KEYRING WROS_UPDATE_SIGNERS SOURCE_DATE_EPOCH CARGO_HOME RUSTUP_HOME CARGO_NET_OFFLINE \
                CARGO_TARGET_DIR WROS_GIT_COMMIT; do
        [[ -z ${!name:-} ]] || keep+=("$name=${!name}")
    done
    runuser -u "$build_user" -- env -i HOME="$build_home" USER="$build_user" LOGNAME="$build_user" \
        PATH="${PATH}" LANG=C.UTF-8 PYTHONDONTWRITEBYTECODE=1 "${keep[@]}" "$@"
}

# ---------------------------------------------------------------- state

state_done() { [[ -e $build/state/$1.done ]]; }
state_mark() { date -u +%Y-%m-%dT%H:%M:%SZ > "$build/state/$1.done"; }

source_fingerprint() {
    as_builder python3 -I -c 'import sys; sys.path.insert(0, sys.argv[1]); import wros_source as s; print(s.fingerprint()["sha256"])' "$repo/tools"
}

run_step() {
    local name=$1
    if state_done "$name"; then log "step $name: already complete (resume)"; return 0; fi
    log "step $name: begin"
    "step_${name//-/_}" 2>&1 | tee "$build/logs/$name.log" >&2
    state_mark "$name"
    log "step $name: done"
}

# ---------------------------------------------------------------- steps

step_binaries() {
    cargo_target=${CARGO_TARGET_DIR:-$build/cargo-target}
    # shellcheck disable=SC2016  # $1/$2 are expanded by the inner bash, from the arguments below
    as_builder env CARGO_TARGET_DIR="$cargo_target" bash -c '
        set -euo pipefail
        cd "$1"
        cargo build --release --locked --no-default-features --features gui-drm -j "$2" -p grimoire-tui -p wros-greeter -p wros-boot -p wros-installer
    ' _ "$repo" "${WROS_CARGO_JOBS:-2}"
    for name in grimoire wros-greeter wros-installer; do
        [[ -x $cargo_target/release/$name && ! -L $cargo_target/release/$name ]] || die "cargo did not produce release/$name"
    done
    (cd "$cargo_target/release" && sha256sum grimoire wros-greeter wros-installer) > "$build/input-binaries.sha256"
}

step_prose_runtime() {
    local -a caches=()
    local cache
    IFS=: read -r -a cache_list <<< "${WROS_PROSE_CACHE:-}"
    for cache in "${cache_list[@]}"; do [[ -z $cache ]] || caches+=(--cache-dir "$cache"); done
    local -a extra=()
    [[ -z ${WROS_PROSE_INCLUDE_SOURCES:-} ]] || extra+=(--include-sources)
    as_builder python3 -I "$repo/tools/prepare-prose-tools.py" --output "$build/prose-runtime" "${caches[@]}" "${extra[@]}"
    [[ -d $build/prose-runtime/runtime ]] || die 'prose runtime was not produced at prose-runtime/runtime'
}

step_inputs() {
    mkdir -p "$inputs"
    chown "$build_user" "$inputs"
    local -a fetch=()
    local url name
    for url in "${INPUT_FILES[@]}"; do
        name=${url##*/}
        if [[ -s $inputs/$name ]]; then log "input $name present"; continue; fi
        if [[ -n ${WROS_INPUT_CACHE:-} && -s $WROS_INPUT_CACHE/$name ]]; then
            as_builder cp -- "$WROS_INPUT_CACHE/$name" "$inputs/$name"; log "input $name from cache"; continue
        fi
        fetch+=("$url")
    done
    if ((${#fetch[@]} != 0)); then
        # The builders verify the pinned SHA-256 of every file below, so a wrong byte fails there.
        as_builder python3 -I - "$inputs" "${fetch[@]}" <<'PY'
import os, shutil, sys, urllib.request
destination, urls = sys.argv[1], sys.argv[2:]
for url in urls:
    target = os.path.join(destination, url.rsplit('/', 1)[1])
    with urllib.request.urlopen(url, timeout=120) as response, open(target + '.part', 'wb') as output:
        shutil.copyfileobj(response, output)
    os.rename(target + '.part', target)
    print('fetched', url)
PY
    fi
}

step_installer_frontend() {
    as_builder bash "$repo/os/scripts/build-installer-frontend.sh" "$inputs/cdebconf_0.280.dsc" "$build/frontend"
}

step_live_backend() {
    as_builder bash "$repo/os/scripts/build-live-backend.sh" \
        "$inputs/debian-installer_20250803+deb13u7.dsc" \
        "$inputs/linux-image-6.12.113+deb13-amd64_6.12.113-1_amd64.deb" \
        "$build/frontend" "$build/backend"
}

step_live_image() {
    # Root: live-build bootstraps and chroots. The update-trust variables pass straight through
    # to install-files.sh. build-installer.sh --live stops after 'lb binary'.
    env GRIMOIRE_BINARY="$cargo_target/release/grimoire" \
        WROS_GREETER_BINARY="$cargo_target/release/wros-greeter" \
        WROS_INSTALLER_BINARY="$cargo_target/release/wros-installer" \
        WROS_PROSE_RUNTIME="$build/prose-runtime/runtime" \
        bash "$repo/os/scripts/build-installer.sh" --live --build-dir "$live"
    [[ -f $live/wros-live-amd64.hybrid.iso.capsule-less ]] || die 'build-installer.sh --live produced no capsule-less ISO'
}

step_native_pool() {
    local lists=$build/backend/source/build/apt.udeb/state/lists
    [[ -d $lists ]] || die "authenticated APT lists missing from the backend build: $lists"
    local -a roots=(--archive-root "$live/binary" --archive-root "$build/backend/source/build/apt.udeb/cache/archives")
    local cache
    IFS=: read -r -a cache_list <<< "${WROS_POOL_CACHE:-}"
    for cache in "${cache_list[@]}"; do [[ -z $cache ]] || roots+=(--archive-root "$cache"); done
    as_builder python3 -I "$repo/tools/build-live-installer-pool.py" --indexes "$lists" "${roots[@]}" \
        --output "$build/native-pool" --download
}

step_native_capsule() {
    as_builder python3 -I "$repo/tools/stage-native-installer.py" \
        --backend "$build/backend" --pool "$build/native-pool" --medium "$live/binary" --output "$build/native-capsule"
}

step_remaster() {
    [[ -f $live/wros-live-amd64.hybrid.iso.capsule-less && ! -e $live/binary/native-installer ]] \
        || die 'remaster preconditions failed (capsule-less ISO missing or capsule already in the medium)'
    cp -a "$build/native-capsule" "$live/binary/native-installer"
    chown -R root:root "$live/binary/native-installer"
    chmod -R u=rwX,go=rX "$live/binary/native-installer"
    # Re-run live-build's own checksum and ISO stages so the El Torito, MBR, GPT and
    # appended-partition options are exactly those of the original image.
    (
        cd "$live"
        rm -f .build/binary_checksums .build/binary_iso
        lb binary_checksums
        lb binary_iso
    )
    local iso=$live/wros-live-amd64.hybrid.iso
    [[ -f $iso && ! -L $iso ]] || die 'lb binary_iso did not produce the re-mastered ISO'
    mkdir -p "$mastered"
    mv "$iso" "$mastered/$IMAGE_NAME-amd64.hybrid.iso"
    chown -R "$build_user" "$mastered"
}

audit_media() {   # exits non-zero unless the real ISO passes; writes the audit JSON
    local iso=$1 json=$2
    local -a args=(--profile live-session --iso "$iso"
        --baseline-iso "$live/wros-live-amd64.hybrid.iso.capsule-less"
        --binary "grimoire=$cargo_target/release/grimoire"
        --binary "wros-greeter=$cargo_target/release/wros-greeter"
        --binary "wros-installer=$cargo_target/release/wros-installer"
        --scratch "$build/audit-scratch"
        --tree-result "$build/tree-audit.json")
    ((interim == 0)) || args+=(--interim)
    rm -rf "$build/audit-scratch" "$build/tree-audit.json"
    # The tree gate reads the root-owned staging chroot (polkitd keeps /etc/polkit-1/rules.d
    # root:polkitd 0750), so it runs as root and read-only; the ISO bytes are audited unprivileged.
    if python3 -I "$repo/tools/audit-installer-media.py" "$live" --tree-only > "$build/tree-audit.json.partial" 2> "$json.err"; then
        chmod 0644 "$build/tree-audit.json.partial"
        mv "$build/tree-audit.json.partial" "$build/tree-audit.json"
    else
        rm -f "$build/tree-audit.json.partial"
        : > "${json%.json}.FAILED.json"
        cp "$json.err" "${json%.json}.FAILED.txt"
        echo "media audit FAILED (tree gate): $(tail -n 3 "$json.err" | tr '\n' ' ')" >&2
        return 1
    fi
    if as_builder python3 -I "$repo/tools/audit-installer-media.py" "$live" "${args[@]}" > "$json.partial" 2> "$json.err"; then
        mv "$json.partial" "$json"
        rm -f "$json.err"
    else
        mv "$json.partial" "${json%.json}.FAILED.json"
        cp "$json.err" "${json%.json}.FAILED.txt"
        echo "media audit FAILED: $(tail -n 3 "$json.err" | tr '\n' ' ')" >&2
        return 1
    fi
}

step_audit() {
    audit_media "$mastered/$IMAGE_NAME-amd64.hybrid.iso" "$build/media-audit.json"
    rm -rf "$build/audit-scratch"
}

# Firmware-to-bootloader smoke boot with no disk, no NIC and a read-only medium. It proves that
# OVMF finds and starts the ISO's UEFI loader from the medium (CD-ROM El Torito entry and the
# hybrid USB/GPT path), that the machine does not reset, and that the display keeps changing
# (GRUB menu -> kernel) rather than sitting in a firmware shell. It is not a boot test: GRUB uses a
# graphical terminal, so no kernel text is available. The VM drivers own real boot qualification.
smoke_boot() {   # mode (cdrom|usb), iso, outdir
    python3 -I - "$1" "$2" "$3" "${WROS_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}" \
        "${WROS_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}" "${WROS_SMOKE_SECONDS:-60}" <<'PY'
import hashlib, json, os, shutil, socket, subprocess, sys, time
mode, iso, outdir, code, vars_template, seconds = sys.argv[1:7]
seconds = int(seconds)
os.makedirs(outdir, exist_ok=True)
log, vars_copy, qmp_path = (os.path.join(outdir, name) for name in (mode + '.serial.log', mode + '.vars.fd', mode + '.qmp'))
shutil.copyfile(vars_template, vars_copy)
accel = 'kvm' if os.access('/dev/kvm', os.R_OK | os.W_OK) else 'tcg'
media = (['-drive', f'file={iso},media=cdrom,readonly=on,if=ide,format=raw'] if mode == 'cdrom' else
         ['-drive', f'if=none,id=stick,file={iso},format=raw,readonly=on', '-device', 'qemu-xhci',
          '-device', 'usb-storage,drive=stick,bootindex=0'])
command = ['qemu-system-x86_64', '-machine', f'q35,accel={accel}', '-m', '2048', '-smp', '2', '-display', 'none',
           '-serial', f'file:{log}', '-monitor', 'none', '-qmp', f'unix:{qmp_path},server=on,wait=off', '-nic', 'none',
           '-no-reboot', '-drive', f'if=pflash,format=raw,unit=0,readonly=on,file={code}',
           '-drive', f'if=pflash,format=raw,unit=1,file={vars_copy}', *media]
process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
def qmp(connection, name, **arguments):
    connection.sendall(json.dumps({'execute': name, 'arguments': arguments}).encode() + b'\n')
    buffer = b''
    while True:
        buffer += connection.recv(65536)
        for line in buffer.splitlines():
            if line.startswith(b'{"return"') or line.startswith(b'{"error"'):
                return json.loads(line)
def frame(connection, name):
    path = os.path.join(outdir, f'{mode}.{name}.ppm')
    reply = qmp(connection, 'screendump', filename=path)
    if 'error' in reply:
        raise RuntimeError(reply['error']['desc'])
    time.sleep(1)
    data = open(path, 'rb').read()
    return hashlib.sha256(data).hexdigest(), any(byte for byte in data[16:])
result = {'mode': mode, 'accel': accel, 'status': 'failed'}
try:
    for _ in range(40):
        if os.path.exists(qmp_path):
            break
        time.sleep(0.25)
    connection = socket.socket(socket.AF_UNIX); connection.settimeout(30); connection.connect(qmp_path)
    connection.recv(65536); qmp(connection, 'qmp_capabilities')
    time.sleep(max(8, seconds // 5))
    first, menu_lit = frame(connection, 'early')
    # The live menu waits without a timeout (live-build's default); choose its default entry
    # as a person would, so the late frame shows whether that entry actually boots.
    reply = qmp(connection, 'send-key', keys=[{'type': 'qcode', 'data': 'ret'}])
    if 'error' in reply:
        raise RuntimeError(reply['error']['desc'])
    time.sleep(seconds)
    last, lit = frame(connection, 'late')
    alive = process.poll() is None
    text = open(log, 'rb').read().decode('latin-1')
    started = f'BdsDxe: starting Boot' in text and ('DVD-ROM' in text if mode == 'cdrom' else 'USB' in text)
    result.update(firmware_started_medium_loader=started, machine_alive=alive, menu_lit=menu_lit,
                  default_entry_selected=True, display_changed=first != last,
                  display_lit=lit, early_frame_sha256=first, late_frame_sha256=last)
    if started and alive and menu_lit and lit and first != last:
        result['status'] = 'passed'
except Exception as error:   # any QMP/qemu failure is a failed smoke boot, never a pass
    result['error'] = str(error)
finally:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(10)
        except subprocess.TimeoutExpired:
            process.kill()
    result['stderr'] = process.stderr.read().decode('utf-8', 'replace')[-400:]
json.dump(result, open(os.path.join(outdir, mode + '.json'), 'w'), indent=2)
print(json.dumps({k: v for k, v in result.items() if k != 'stderr'}))
sys.exit(0 if result['status'] == 'passed' else 1)
PY
}

step_smoke_boot() {
    local iso=$mastered/$IMAGE_NAME-amd64.hybrid.iso mode
    mkdir -p "$build/smoke"
    for mode in cdrom usb; do
        smoke_boot "$mode" "$iso" "$build/smoke" || {
            echo "smoke boot ($mode) failed; see $build/smoke/$mode.json and $mode.serial.log" >&2; return 1; }
    done
}

# ---------------------------------------------------------------- publication

release_name() {  # version, interim -> release basename
    local safe=${1//[~]/-}
    safe=${safe//[^A-Za-z0-9.+-]/_}
    if (($2 != 0)); then printf 'wros-%s-interim-amd64' "$safe"; else printf 'wros-%s-amd64' "$safe"; fi
}

write_receipt() {  # out version_source name iso audit_json
    python3 -I - "$@" <<'PY'
import hashlib, json, os, sys
out, version, version_source, name, iso, audit_path, smoke_dir, started, interim, resumed, drift, driver, steps = sys.argv[1:14]
audit = json.load(open(audit_path))
def digest(path):
    with open(path, 'rb') as stream:
        if hasattr(hashlib, 'file_digest'):
            return hashlib.file_digest(stream, 'sha256').hexdigest()
        hasher = hashlib.sha256()
        while chunk := stream.read(65536):
            hasher.update(chunk)
        return hasher.hexdigest()
iso_sha = digest(iso)
if audit['iso']['sha256'] != iso_sha:
    raise SystemExit('audited ISO differs from the ISO being published')
smoke = {m: json.load(open(os.path.join(smoke_dir, m + '.json'))) for m in ('cdrom', 'usb')}
receipt = {
    'schema_version': 1,
    'release_candidate': interim == '0',
    'name': name, 'version': version, 'version_source': version_source,
    'iso': {'file': name + '.iso', 'sha256': iso_sha, 'bytes': os.path.getsize(iso)},
    'git_commit': audit['git_commit'], 'git_dirty': audit['git_dirty'],
    'source_fingerprint': audit['source_fingerprint'],
    'input_binaries_sha256': audit['input_binaries_sha256'],
    'binaries': audit.get('binaries'),
    'kernel_abi': audit['squashfs']['kernel_abi'],
    'capsule': audit['capsule'], 'update_trust': audit['update_trust'],
    'media_audit_sha256': digest(audit_path),
    'boot_tested': False,
    'smoke_boot': smoke,
    'firmware': 'UEFI x86-64 with Secure Boot disabled; legacy BIOS and Secure Boot are not claimed',
    'secure_boot_tested': False,
    'build': {'started_utc': started, 'source_date_epoch': os.environ.get('SOURCE_DATE_EPOCH'),
              'driver_sha256': digest(driver), 'resumed': resumed == '1',
              'source_drift_accepted': drift == '1', 'steps': steps.split(',')},
    'scope': 'Static audit of the finished ISO plus a firmware-to-GRUB smoke boot. boot_tested is false: '
             'installation, first boot, update and rescue are qualified separately by the VM drivers.',
}
with open(out, 'w') as stream:
    json.dump(receipt, stream, indent=2)
    stream.write('\n')
PY
}

publish_release() {  # candidate_iso audit_json release_dir name version version_source
    local candidate=$1 audit_json=$2 out=$3 name=$4 version=$5 version_source=$6
    [[ -f $audit_json && -f $candidate ]] || die 'refusing to publish: audit result or candidate ISO missing'
    python3 -I -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("passed") is True else 1)' "$audit_json" \
        || die 'refusing to publish: the media audit did not pass'
    mkdir -p "$out"
    local final=$out/$name
    local f
    for f in "$final.iso" "$final.iso.sha256" "$final.receipt.json"; do
        [[ ! -e $f && ! -L $f ]] || die "refusing to overwrite finished release artifact: $f"
    done
    cp --reflink=auto -- "$candidate" "$final.iso.partial"
    local sha
    sha=$(sha256sum "$final.iso.partial" | cut -d' ' -f1)
    python3 -I -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1]))["iso"]["sha256"]==sys.argv[2] else 1)' "$audit_json" "$sha" \
        || die 'refusing to publish: the ISO bytes are not the audited bytes'
    printf '%s  %s\n' "$sha" "$name.iso" > "$final.iso.sha256.partial"
    write_receipt "$final.receipt.json.partial" "$version" "$version_source" "$name" "$final.iso.partial" "$audit_json" \
        "$build/smoke" "$started" "$interim" "$resumed" "$drift" "${BASH_SOURCE[0]}" "$(IFS=,; echo "${STEPS[*]}")"
    # The receipt is renamed last: its existence implies the ISO and checksum are final.
    mv -- "$final.iso.partial" "$final.iso"
    mv -- "$final.iso.sha256.partial" "$final.iso.sha256"
    mv -- "$final.receipt.json.partial" "$final.receipt.json"
    published_iso=$final.iso
    published_sha=$sha
}

step_publish() {
    publish_release "$mastered/$IMAGE_NAME-amd64.hybrid.iso" "$build/media-audit.json" "$release_dir" "$release_base" \
        "$version" "$version_source"
    log "published $published_iso"
    log "sha256 $published_sha"
    log "Scope: statically audited ISO plus a firmware-to-GRUB smoke boot (UEFI, Secure Boot disabled). Not yet qualified by installation, update or rescue VM runs, or on hardware."
}

# ---------------------------------------------------------------- main

main() {
    local resume=0 plan=0 preflight_only=0
    interim=0; drift=0; resumed=0
    local allow_dirty=0 build_arg='' output_arg=''
    while (($# != 0)); do
        case $1 in
            --build-dir) [[ $# -ge 2 && $2 == /* ]] || die '--build-dir needs an absolute path' 2; build_arg=$2; shift 2 ;;
            --output-dir) [[ $# -ge 2 && $2 == /* ]] || die '--output-dir needs an absolute path' 2; output_arg=$2; shift 2 ;;
            --resume) resume=1; shift ;;
            --accept-source-drift) drift=1; shift ;;
            --interim) interim=1; shift ;;
            --allow-dirty) allow_dirty=1; shift ;;
            --plan) plan=1; shift ;;
            --preflight-only) preflight_only=1; shift ;;
            -h|--help) usage; exit 0 ;;
            *) usage; exit 2 ;;
        esac
    done
    if ((plan)); then printf '%s\n' "${STEPS[@]}"; return 0; fi
    ((drift == 0 || resume == 1)) || die '--accept-source-drift only applies to --resume' 2
    ((allow_dirty == 0 || interim == 1)) || die '--allow-dirty is only for --interim builds' 2
    preflight || return 2
    ((preflight_only == 0)) || { log 'preflight passed'; return 0; }
    preflight_inputs

    started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    version_source=os/VERSION
    if [[ -f $repo/os/VERSION ]]; then
        version=$(head -n 1 "$repo/os/VERSION" | tr -d '\r')
    else
        version=$FALLBACK_VERSION
        version_source="fallback (os/VERSION is not present in this tree; using $FALLBACK_VERSION)"
        log "NOTE: $version_source"
    fi
    [[ $version =~ ^[0-9][A-Za-z0-9.+~-]*$ ]] || die "invalid version in os/VERSION: $version" 2

    build=${build_arg:-$repo/build/release-iso/$(date -u +%Y%m%dT%H%M%SZ)}
    inputs=${WROS_INPUT_CACHE:-$build/inputs}
    [[ -z ${WROS_INPUT_CACHE:-} ]] && inputs=$build/inputs
    live=$build/live
    mastered=$build/mastered
    release_dir=${output_arg:-$build/release}
    release_base=$(release_name "$version" "$interim")
    [[ ! -L $build ]] || die 'refusing a symlinked build directory' 2
    if ((resume)); then
        [[ -d $build/state ]] || die "--resume needs an existing build directory with state: $build" 2
        resumed=1
    else
        [[ ! -e $build ]] || die "build directory exists (finished artifacts are never reused or deleted): $build; use --resume or a new --build-dir" 2
        install -d -m 755 -o "$build_user" "$build"
        install -d -m 755 "$build/state" "$build/logs"
    fi
    commit=$(git -C "$repo" rev-parse HEAD) || die 'the build source must be a git checkout' 2
    if [[ -n $(git -C "$repo" status --porcelain --untracked-files=no) ]] && ((allow_dirty == 0)); then
        die 'the working tree has uncommitted changes; commit them (or use --interim --allow-dirty)' 2
    fi
    if [[ -z ${SOURCE_DATE_EPOCH:-} ]]; then
        SOURCE_DATE_EPOCH=$(git -C "$repo" log -1 --format=%ct)
    fi
    export SOURCE_DATE_EPOCH
    fingerprint=$(source_fingerprint)
    if ((resume)); then
        if [[ $(cat "$build/state/source-fingerprint") != "$fingerprint" ]] && ((drift == 0)); then
            die 'the source changed since this build started; refusing to resume (--accept-source-drift records the drift in the receipt)' 2
        fi
    else
        printf '%s\n' "$fingerprint" > "$build/state/source-fingerprint"
        printf '%s\n' "$commit" > "$build/state/git-commit"
    fi
    export started interim resumed drift
    cargo_target=${CARGO_TARGET_DIR:-$build/cargo-target}
    log "build directory: $build (commit $commit, version $version${interim:+, interim=$interim})"
    log "update trust inputs: keyring=$WROS_UPDATE_KEYRING signers=$WROS_UPDATE_SIGNERS"
    local step
    for step in "${STEPS[@]}"; do
        run_step "$step"
    done
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
