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
