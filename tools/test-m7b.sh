#!/usr/bin/env bash
# Unified offline gates and source-aligned evidence. Provision inputs first.
set -euo pipefail
cd "$(dirname "$0")/.."
export WRITINGDECK_TOOLS_PYTHON=${WRITINGDECK_TOOLS_PYTHON:-$PWD/build/prose-runtime/runtime/bin/python3}
export GRIMOIRE_BINARY=${GRIMOIRE_BINARY:-${CARGO_TARGET_DIR:-$PWD/target}/debug/grimoire}
exec python3 tools/verify-writingdeck.py --milestone 'M7b offline prose, WordNet, voice and revision software'
