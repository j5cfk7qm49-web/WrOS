#!/usr/bin/env bash
# One command for a release candidate's checks (#13), with a summary at the end.
#
#   tools/rc-check.sh                 format, lint, Rust, the app in a terminal, Python
#   tools/rc-check.sh --vm CMD...     also run a VM chain command and count it
#
# Run as the normal user in the build environment (never root: the screen tests
# refuse to run the writing app as root). Logs go to $RC_CHECK_LOGS (default
# target/rc-check); the summary is printed and kept as summary.txt beside them.
set -uo pipefail
cd "$(dirname "$0")/.."

logs=${RC_CHECK_LOGS:-${CARGO_TARGET_DIR:-$PWD/target}/rc-check}
mkdir -p "$logs"
target=${CARGO_TARGET_DIR:-$PWD/target}
cargo=(cargo --offline --locked)
summary=()
failed=0

# step NAME COMMAND...: runs it, keeps its log, and records one summary line.
step() {
    local name=$1 log="$logs/$1.log" start=$SECONDS
    shift
    printf '== %s\n' "$name"
    if "$@" >"$log" 2>&1; then
        summary+=("PASS  $name  ($((SECONDS - start)) s)  $(counts "$log")")
    else
        summary+=("FAIL  $name  ($((SECONDS - start)) s)  $(counts "$log")  see $log")
        failed=1
    fi
}

# Test totals from cargo's and unittest's own lines.
counts() {
    local passed failed_n ran
    passed=$(grep -o '[0-9]* passed' "$1" | awk '{s += $1} END {print s + 0}')
    failed_n=$(grep -o '[0-9]* failed' "$1" | awk '{s += $1} END {print s + 0}')
    ran=$(grep -o '^Ran [0-9]*' "$1" | awk '{s += $2} END {print s + 0}')
    if [ "$ran" -gt 0 ]; then
        printf '%s tests, %s' "$ran" "$(grep -E '^(OK|FAILED)' "$1" | tail -1)"
    elif [ "$passed$failed_n" != 00 ]; then
        printf '%s passed, %s failed' "$passed" "$failed_n"
    fi
}

# cargo-fmt and cargo-clippy by name: the same with rustup or with a toolchain installed alone.
step format cargo-fmt fmt --all --check
step clippy cargo-clippy clippy --offline --locked --no-default-features --workspace --all-targets -- -D warnings
step rust "${cargo[@]}" test --no-default-features --workspace --no-fail-fast
step app-in-terminal "${cargo[@]}" test --no-default-features --features test-driver --test driven
# The Python screen tests drive the debug binary; build it so they run instead of skipping.
step build "${cargo[@]}" build --workspace --no-default-features --features grimoire-tui/pty-qa
step python env GRIMOIRE_BINARY="$target/debug/grimoire" WROS_QA_PTY=1 \
    python3 -m unittest discover -s os/tests -p 'test_*.py'

if [ "${1:-}" = --vm ]; then
    shift
    step vm-chain "$@"
fi

{
    printf '\nWrOS checks at %s (%s)\n' "$(git rev-parse --short HEAD 2>/dev/null)" "$(date -u +%FT%TZ)"
    printf '%s\n' "${summary[@]}"
} | tee "$logs/summary.txt"
exit "$failed"
