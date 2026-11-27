#!/usr/bin/env python3
"""List WrOS-only Cargo packages or check protected versions against Grimoire."""

import argparse
from pathlib import Path
import subprocess
import sys
import tomllib


def compatibility_key(name, version):
    # Cargo permits a caret update up to the leftmost non-zero component.
    major, minor, patch = version.split("+", 1)[0].split("-", 1)[0].split(".")
    if major != "0":
        return name, major
    if minor != "0":
        return name, major, minor
    return name, major, minor, patch


def locked_versions(packages):
    versions = {}
    for package in packages:
        name, version = package["name"], package["version"]
        versions.setdefault(compatibility_key(name, version), set()).add(version)
    return versions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify upstream versions")
    parser.add_argument("--lock", type=Path, default=Path("Cargo.lock"))
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    upstream_lock = subprocess.check_output(
        ["git", "show", "upstream/main:Cargo.lock"], cwd=root, encoding="utf-8",
    )
    upstream = locked_versions(tomllib.loads(upstream_lock)["package"])
    local_packages = tomllib.loads(args.lock.read_text(encoding="utf-8"))["package"]

    if not args.check:
        packages = set()
        for package in local_packages:
            name, version = package["name"], package["version"]
            if "source" in package and compatibility_key(name, version) not in upstream:
                packages.add(f"{name}@{version}")
        for package in sorted(packages):
            print(package)
        return 0

    local = locked_versions(local_packages)
    mismatches = 0
    for key, expected in sorted(upstream.items()):
        actual = local.get(key, set())
        if actual != expected:
            mismatches += 1
            print(f"{key[0]}: expected {', '.join(sorted(expected))}; "
                  f"found {', '.join(sorted(actual)) or 'missing'}", file=sys.stderr)
    if mismatches:
        return 1
    print(f"All {sum(map(len, upstream.values()))} upstream package versions match.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
