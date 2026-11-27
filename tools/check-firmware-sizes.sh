#!/usr/bin/env bash
# Build only the firmware outputs, then check the NAR sizes of the system's payloads.
set -euo pipefail

flake=${1:-.}
names=$(nix eval --raw "$flake#nixosConfigurations.wros.config.wros.firmware" \
  --apply 'packages: builtins.concatStringsSep "\n" (builtins.attrNames packages)')
targets=()
while IFS= read -r driver; do
  targets+=("$flake#wros-firmware-$driver")
done <<< "$names"

paths=$(nix build --no-link --print-out-paths "${targets[@]}")
mapfile -t outputs <<< "$paths"
sizes=$(nix path-info -S "${outputs[@]}")
printf '%s\n' "$sizes"
awk '$NF >= 95000000 { print "Firmware exceeds the 95 MB budget:", $0 > "/dev/stderr"; failed=1 }
     END { exit failed }' <<< "$sizes"
