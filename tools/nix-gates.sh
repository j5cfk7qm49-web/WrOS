#!/usr/bin/env bash
# The repository gates Nix CI runs from a checkout, inside `nix develop`: formatting, retained
# upstream sources, content and documentation checks, shell and Python syntax. The tests
# themselves are the flake's checks (rust-suite*, app-suite, python-suite).
set -euo pipefail
cd "$(dirname "$0")/.."
step() { printf '== %s\n' "$*"; "$@"; }

step cargo fetch --locked --quiet
step cargo fmt --all -- --check
step python3 tools/audit-terminal-library.py > /dev/null
step python3 tools/build-spark-corpus.py --check
step python3 tools/audit-writing-checks-assets.py
step shellcheck os/scripts/*.sh os/config/installer/native-crypto-passphrase tools/nix-gates.sh
python_sources=(os/tests/*.py tools/*.py os/scripts/install-plan.py os/scripts/wros-keyboard
  os/scripts/wros-display os/scripts/wros-lock os/scripts/wros-backup os/scripts/wros-sync
  os/scripts/wros-update os/scripts/wros-session nixos/scripts/wros-install-core
  nixos/scripts/wros-installer-backend nixos/scripts/wros-install-session
  nixos/scripts/wros-live-session nixos/scripts/wros-update-engine.py)
step python3 -m py_compile "${python_sources[@]}"
step python3 tools/check-wros.py
echo "all gates passed"
