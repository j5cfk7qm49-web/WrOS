#!/usr/bin/env python3
"""Choose and download the Debian packages an update bundle carries with the wros package (#117).

The bundle installs offline, so it carries every Debian package the new wros package needs that
the installation being upgraded lacks or has at an older version. Those packages come from
Debian 13 with its updates and security archives, and each must be the newest Debian 13 version:
an installation that took a Debian security update must never be offered an older one.

    collect-update-runtime.py WROS_DEB BASELINE_DPKG_STATUS WORK_DIR

BASELINE_DPKG_STATUS is /var/lib/dpkg/status from the saved installation being upgraded. WORK_DIR
must not exist; on success WORK_DIR/runtime/ holds the chosen archives, WORK_DIR/apt/ the private
APT state, and the exact offline plan for the baseline has been proved with the update engine.
Pass every archive in WORK_DIR/runtime/ to tools/build-update-bundle.py after the wros package.
"""
import argparse
import importlib.machinery
import importlib.util
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCES = (
    'deb https://deb.debian.org/debian trixie main\n'
    'deb https://deb.debian.org/debian trixie-updates main\n'
    'deb https://security.debian.org/debian-security trixie-security main\n'
)
INST = re.compile(r'^Inst ([a-z0-9+.:~-]+)(?: \[[^\]]+\])? \((\S+)', re.M)


def load_engine():
    path = ROOT / 'os/scripts/wros-update-engine.py'
    loader = importlib.machinery.SourceFileLoader('wros_update_engine', str(path))
    engine = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(engine)
    return engine


def apt_options(apt):
    return ['-o', f'Dir::Etc::sourcelist={apt / "sources.list"}', '-o', 'Dir::Etc::sourceparts=-',
            '-o', f'Dir::State::lists={apt / "lists"}', '-o', f'Dir::Cache={apt / "cache"}',
            '-o', 'Debug::NoLocking=1']


def planned(simulation):
    """{package: version} for each Inst line of an APT simulation, wros left out."""
    plan = dict(INST.findall(simulation))
    plan.pop('wros', None)
    return plan


def stale(candidates, chosen):
    """Archives older than Debian 13's newest version: [(package, chosen, newest)]."""
    return sorted((name, version, candidates.get(name)) for name, version in chosen.items()
                  if candidates.get(name) != version)


def deb_field(archive, *names):
    # Asked for one field, dpkg-deb prints the bare value; so read the whole control file instead.
    out = subprocess.check_output(['/usr/bin/dpkg-deb', '--field', str(archive)], text=True)
    fields = dict(line.split(': ', 1) for line in out.splitlines()
                  if ': ' in line and not line[:1].isspace())
    return [fields[name] for name in names]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('wros_deb', type=Path)
    parser.add_argument('baseline_status', type=Path)
    parser.add_argument('work', type=Path)
    args = parser.parse_args(argv)
    deb = args.wros_deb.resolve(strict=True)
    if deb_field(deb, 'Package')[0] != 'wros':
        parser.error('the first argument must be the wros package')
    work = args.work.resolve()
    work.mkdir()
    apt, runtime, dpkg = work / 'apt', work / 'runtime', work / 'dpkg'
    for directory in (apt / 'lists/partial', apt / 'cache/archives/partial', runtime, dpkg):
        directory.mkdir(parents=True)
    (apt / 'sources.list').write_text(SOURCES)
    (dpkg / 'status').write_bytes(args.baseline_status.read_bytes())
    options = apt_options(apt)
    subprocess.run(['/usr/bin/apt-get', *options, 'update', '-qq'], check=True)

    simulation = subprocess.run(['/usr/bin/apt-get', *options, '--simulate', '--no-remove',
                                 '--no-install-recommends', '-o', f'Dir::State::status={dpkg / "status"}',
                                 'install', str(deb)], capture_output=True, text=True)
    (work / 'debian-plan.log').write_text(simulation.stdout + simulation.stderr)
    if simulation.returncode:
        raise SystemExit(simulation.stderr.strip() or 'APT could not plan the update')
    chosen = planned(simulation.stdout)
    print('Debian packages for the bundle:', ' '.join(f'{n}={v}' for n, v in sorted(chosen.items())) or 'none',
          flush=True)
    if chosen:
        subprocess.run(['/usr/bin/apt-get', *options, 'download',
                        *(f'{n}={v}' for n, v in sorted(chosen.items()))], cwd=runtime, check=True)

    archives = sorted(runtime.glob('*.deb'))
    downloaded = dict(deb_field(a, 'Package', 'Version') for a in archives)
    if downloaded != chosen:
        raise SystemExit('the downloaded archives are not the planned packages')
    candidates = {}
    for name in chosen:
        policy = subprocess.check_output(['/usr/bin/apt-cache', *options, 'policy', name], text=True)
        found = re.search(r'Candidate: (\S+)', policy)
        candidates[name] = found.group(1) if found else None
    old = stale(candidates, chosen)
    if old:
        raise SystemExit('not the newest Debian 13 version: ' +
                         ', '.join(f'{n} {v} (newest {c})' for n, v, c in old))

    engine = load_engine()
    metadata = [{'name': n, 'version': v} for n, v in
                (deb_field(a, 'Package', 'Version') for a in [deb, *archives])]
    resolved = engine.plan_with_apt([deb, *archives], metadata, dpkg, work / 'apt-offline')
    print('Exact offline plan verified:', len(resolved), 'packages', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
