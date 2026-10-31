"""Identity policy for the one fresh target in a marked QEMU test.

This module performs metadata reads only. It is not an installed storage API;
neither callers nor command-line arguments can nominate a host disk. The host
fixture creates its own image and assigns the nonce-bearing virtio serial.
"""
from pathlib import Path
import base64
import re

TARGET_BYTES = 16 * 1024**3
BLOCK_NAME = re.compile(r'[A-Za-z0-9_.-]{1,128}')


def require(value, message):
    if not value:
        raise ValueError(message)


def field(path, limit=512):
    with path.open('r', encoding='ascii') as stream:
        value = stream.read(limit+1)
    require(0 < len(value) <= limit, 'Invalid fixture device metadata')
    return value.strip()


def device_number(path):
    value = field(path/'dev')
    require(re.fullmatch(r'[0-9]{1,10}:[0-9]{1,10}', value) is not None,
            'Invalid fixture device number')
    values = tuple(map(int, value.split(':')))
    require(all(0 <= value < 2**32 for value in values), 'Invalid fixture device number')
    return values


def fixture_serial(token):
    require(isinstance(token, str) and re.fullmatch('[0-9a-f]{32}', token) is not None,
            'Invalid target fixture marker')
    # virtio-blk exposes a 20-byte identifier. Base85 preserves all 128 nonce
    # bits in exactly 20 printable bytes, without a QEMU option separator.
    return base64.b85encode(bytes.fromhex(token)).decode('ascii')


def inspect_target(description, token, sysroot=Path('/sys/class/block')):
    expected = {'bytes': TARGET_BYTES, 'serial': fixture_serial(token)}
    require(type(description) is dict and description == expected
            and type(description.get('bytes')) is int,
            'Only the fixed freshly generated target description is accepted')
    target = sysroot/'vda'
    require(target.is_dir() and field(target/'ro') == '0', 'Fixture target is not writable')
    size = field(target/'size')
    require(size.isascii() and size.isdigit() and int(size)*512 == TARGET_BYTES,
            'Fixture target capacity differs')
    serials = [field(path) for path in (target/'serial', target/'device/serial') if path.is_file()]
    require(serials and set(serials) == {expected['serial']}, 'Fixture target serial differs')
    require(not (target/'partition').exists(), 'Fixture target must be the whole generated disk')
    major, minor = device_number(target)
    return {'name': 'vda', 'bytes': TARGET_BYTES, 'serial': expected['serial'],
            'major': major, 'minor': minor}


def belongs_to_target(name, identity, sysroot=Path('/sys/class/block')):
    """Accept only this disk, its actual partitions and exclusive descendants."""
    target = sysroot/'vda'
    require(identity.get('name') == 'vda' and identity.get('bytes') == TARGET_BYTES,
            'Invalid inspected fixture target')
    require(device_number(target) == (identity['major'], identity['minor']),
            'Fixture target identity changed')
    serials = [field(path) for path in (target/'serial', target/'device/serial') if path.is_file()]
    require(serials and set(serials) == {identity['serial']}, 'Fixture target identity changed')
    size = field(target/'size')
    require(size.isascii() and size.isdigit() and int(size)*512 == identity['bytes'],
            'Fixture target capacity changed')
    visited = set()

    def inspect(block, ancestors):
        require(isinstance(block, str) and block not in ('.', '..') and BLOCK_NAME.fullmatch(block) is not None,
                'Invalid fixture block name')
        require(len(ancestors) < 32 and block not in ancestors, 'Cyclic or excessive fixture dependencies')
        visited.add(block)
        require(len(visited) <= 512, 'Excessive fixture dependencies')
        path = sysroot/block
        require(path.is_dir(), 'Fixture dependency is absent')
        if block == 'vda':
            return device_number(path) == (identity['major'], identity['minor'])
        if (path/'partition').exists():
            # sysfs puts a real partition below its owning whole-disk device.
            return path.resolve().parent == target.resolve()
        slaves = path/'slaves'
        if not slaves.is_dir():
            return False
        dependencies = sorted(item.name for item in slaves.iterdir())
        return bool(dependencies) and all(inspect(item, ancestors | {block}) for item in dependencies)

    return inspect(name, set())


def decode_proc_path(token):
    return re.sub(r'\\([0-7]{3})', lambda m: chr(int(m.group(1), 8)), token)


def target_mountpoints(mountinfo_text, target_root='/run/live/installer/target'):
    """Return active mountpoints at or below target_root, ordered deepest-first."""
    require(isinstance(target_root, str) and target_root.startswith('/') and target_root != '/',
            'Invalid installer target root')
    prefix = target_root.rstrip('/')
    mounts = []
    for line in mountinfo_text.splitlines():
        if not line.strip():
            continue
        parts = line.split()
        require(len(parts) >= 5, 'Malformed mountinfo line')
        mountpoint = decode_proc_path(parts[4])
        if mountpoint == prefix or mountpoint.startswith(prefix + '/'):
            mounts.append(mountpoint)
    # Sort deepest path first; preserve reverse discovery order for identical mountpoints.
    indexed = list(enumerate(mounts))
    indexed.sort(key=lambda item: (-len(Path(item[1]).parts), -len(item[1]), -item[0]))
    return [item[1] for item in indexed]


def target_mapper_teardown_order(identity, sysroot=Path('/sys/class/block')):
    """Return target-owned dm-* devices ordered leaf-holders-first (LVM before LUKS)."""
    # Verify target identity even if no dm-* devices exist yet.
    belongs_to_target('vda', identity, sysroot)
    dm_blocks = {}
    for entry in sorted(sysroot.iterdir(), key=lambda p: p.name):
        if not re.fullmatch(r'dm-[0-9]+', entry.name):
            continue
        if not belongs_to_target(entry.name, identity, sysroot):
            continue
        dm_name_path = entry/'dm/name'
        require(dm_name_path.is_file(), f'Missing dm/name metadata for {entry.name}')
        dm_name = field(dm_name_path, 128)
        require(BLOCK_NAME.fullmatch(dm_name) is not None, f'Invalid dm/name for {entry.name}')
        dm_uuid = field(entry/'dm/uuid', 256) if (entry/'dm/uuid').is_file() else ''
        if dm_uuid.startswith('CRYPT-'):
            kind = 'crypt'
        elif dm_uuid.startswith('LVM-'):
            kind = 'lvm'
        else:
            kind = 'dm'
        slaves_dir = entry/'slaves'
        dm_slaves = sorted(
            s.name for s in slaves_dir.iterdir()
            if re.fullmatch(r'dm-[0-9]+', s.name)
        ) if slaves_dir.is_dir() else []
        dm_blocks[entry.name] = {
            'block': entry.name,
            'dm_name': dm_name,
            'dm_uuid': dm_uuid,
            'kind': kind,
            'dm_slaves': dm_slaves,
        }
    # Build holder counts: a device with 0 remaining active holders can be removed next.
    holders_of = {name: set() for name in dm_blocks}
    for name, info in dm_blocks.items():
        for slave in info['dm_slaves']:
            if slave in holders_of:
                holders_of[slave].add(name)
    ordered = []
    remaining = dict(dm_blocks)
    while remaining:
        leaves = sorted(name for name in remaining if not holders_of[name])
        require(leaves, 'Cyclic device-mapper holder graph on target')
        for leaf in leaves:
            info = remaining.pop(leaf)
            for slave in info['dm_slaves']:
                if slave in holders_of:
                    holders_of[slave].discard(leaf)
            ordered.append({
                'block': info['block'],
                'dm_name': info['dm_name'],
                'dm_uuid': info['dm_uuid'],
                'kind': info['kind'],
            })
    return ordered


def target_swaps(swaps_text, identity, sysroot=Path('/sys/class/block')):
    """Return active swap devices in /proc/swaps that belong exclusively to the target."""
    belongs_to_target('vda', identity, sysroot)
    dm_by_mapper_name = {}
    for entry in sysroot.iterdir():
        if re.fullmatch(r'dm-[0-9]+', entry.name) and (entry/'dm/name').is_file():
            dm_by_mapper_name[field(entry/'dm/name', 128)] = entry.name
    swaps = []
    for index, line in enumerate(swaps_text.splitlines()):
        if not line.strip() or (index == 0 and line.startswith('Filename')):
            continue
        parts = line.split()
        require(len(parts) >= 2, 'Malformed /proc/swaps entry')
        swap_path = decode_proc_path(parts[0])
        if swap_path.startswith('/dev/mapper/'):
            mapper_name = swap_path.removeprefix('/dev/mapper/')
            block = dm_by_mapper_name.get(mapper_name)
            require(block is not None, f'Unknown mapper swap device: {swap_path}')
        elif swap_path.startswith('/dev/'):
            block = swap_path.removeprefix('/dev/')
        else:
            raise ValueError(f'Unsupported swap path in fixture: {swap_path}')
        require(belongs_to_target(block, identity, sysroot),
                f'Refusing non-target swap device: {swap_path}')
        swaps.append({'path': swap_path, 'block': block})
    return swaps


def teardown_plan(identity, mountinfo_text, swaps_text='',
                  target_root='/run/live/installer/target',
                  sysroot=Path('/sys/class/block')):
    """Construct a complete, ordered teardown plan for target mounts, swaps, and dm devices."""
    return {
        'mounts': target_mountpoints(mountinfo_text, target_root=target_root),
        'swaps': target_swaps(swaps_text, identity, sysroot=sysroot),
        'mappers': target_mapper_teardown_order(identity, sysroot=sysroot),
    }
