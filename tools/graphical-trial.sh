#!/bin/sh
# Run from the source checkout. All writing goes to a temporary demo book.
set -eu
exec python3 "$(dirname "$0")/graphical_trial.py" "$@"
