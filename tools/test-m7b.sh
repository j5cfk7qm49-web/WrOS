#!/usr/bin/env bash
# Software qualification only. Provision reviewed dependencies first; no downloads.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ $EUID != 0 ]] || { echo 'Run tests as a non-root user' >&2; exit 2; }
mkdir -p build/verification/m7b
export WRITINGDECK_TOOLS_PYTHON=${WRITINGDECK_TOOLS_PYTHON:-$PWD/build/prose-runtime/runtime/bin/python3}
export GRIMOIRE_BINARY=${GRIMOIRE_BINARY:-${CARGO_TARGET_DIR:-$PWD/target}/debug/grimoire}
run_gate() {
    local name=$1
    shift
    echo "Running $name"
    if "$@" > "build/verification/m7b/$name.log" 2>&1; then
        echo "PASS $name"
    else
        local status=$?
        tail -80 "build/verification/m7b/$name.log" >&2
        return "$status"
    fi
}
run_gate format cargo fmt --all -- --check
run_gate corpus python3 tools/build-spark-corpus.py --check
run_gate harper-assets python3 tools/audit-writing-checks-assets.py
run_gate prose-assets python3 tools/audit-prose-tools.py --runtime "$(dirname "$(dirname "$WRITINGDECK_TOOLS_PYTHON")")"
run_gate default cargo test --workspace --locked --offline -j2
run_gate clippy-default cargo clippy --workspace --all-targets --locked --offline -j2 -- -D warnings
run_gate no-audio cargo test --workspace --no-default-features --locked --offline -j2
run_gate clippy-no-audio cargo clippy --workspace --all-targets --no-default-features --locked --offline -j2 -- -D warnings
run_gate build cargo build --workspace --no-default-features --locked --offline -j2
run_gate python python3 -m unittest discover -s os/tests -p 'test_*.py' -v
python3 - <<'PY'
from pathlib import Path
import re
text = Path('build/verification/m7b/python.log').read_text()
if not re.search(r'Ran \d+ tests in ', text) or '\nOK\n' not in text or re.search(r'\.\.\. skipped ', text):
    raise SystemExit('Missing or skipped Python integration coverage is not a passing gate')
PY
run_gate shellcheck shellcheck os/scripts/*.sh os/scripts/writingdeck-session tools/test-m7b.sh
run_gate docs python3 tools/check-writingdeck.py
run_gate whitespace git diff --check
echo 'Software gates passed. VM, physical hardware, provider, and release gates remain separate.'
