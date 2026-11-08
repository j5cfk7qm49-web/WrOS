#!/usr/bin/env bash
# Unified offline gates and source-aligned evidence. Provision inputs first.
set -euo pipefail
cd "$(dirname "$0")/.."
export WROS_TOOLS_PYTHON=${WROS_TOOLS_PYTHON:-$PWD/build/prose-runtime/runtime/bin/python3}
# The lock test drives the pty-qa hook in the debug build verify-wros.py makes.
export WROS_QA_PTY=1
export GRIMOIRE_BINARY=${GRIMOIRE_BINARY:-${CARGO_TARGET_DIR:-$PWD/target}/debug/grimoire}
exec python3 tools/verify-wros.py --milestone 'WrOS integrated writing, native shell and system-adapter software'
