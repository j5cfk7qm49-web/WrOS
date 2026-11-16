#!/usr/bin/env bash
# Backwards-compatible old script name: WrOS is now the official product identity.
set -euo pipefail
exec "$(dirname "$0")/push-wros-source.sh" "$@"
