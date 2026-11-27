#!/usr/bin/env bash
# Used by the wros-firmware-lists derivation. Write one sorted list to stdout.
set -euo pipefail
export LC_ALL=C
shopt -s globstar nullglob

modules_dir=$1
firmware_dir=$2
driver=$3
module_glob=$4
fallback=$5
also=${6:-}

cd "$modules_dir"
# Expand the module pattern from the Nix driver table without splitting on spaces.
IFS=
# shellcheck disable=SC2206
modules=( */kernel/drivers/$module_glob )
requests=
if (( ${#modules[@]} )); then
  requests=$(modinfo -F firmware "${modules[@]}" | sed '/^name:/d; /^$/d' | sort -u)
fi

cd "$firmware_dir"
files=()

# The highest plain (untagged) API number any request names, such as 100 in
# iwlwifi-so-a0-gf-a0-100.ucode.
plain_max=$(sed -nE 's/^.*-([0-9]+)\.[a-z]+$/\1/p' <<< "$requests" | sort -n | tail -1)
plain_max=${plain_max:-0}

# newest_older PREFIX TAG WANTED EXT: for each firmware name matching PREFIX (a glob) that ships
# versions PREFIX TAG <n> EXT, add the newest with n <= WANTED, and set found=true.
newest_older() {
  local glob=$1 tag=$2 wanted=$3 ext=$4 file name api
  local -A best=() best_api=()
  # The glob is expanded on purpose; IFS is empty above, so it isn't split.
  # shellcheck disable=SC2231
  for file in $glob"$tag"*"$ext"; do
    api=${file##*-"$tag"}
    api=${api%"$ext"}
    [[ -f $file && $api =~ ^[0-9]+$ ]] || continue
    (( 10#$api <= 10#$wanted )) || continue
    name=${file%-"$tag$api$ext"}
    if (( 10#$api > ${best_api[$name]:--1} )); then
      best[$name]=$file best_api[$name]=$((10#$api))
    fi
  done
  for name in "${!best[@]}"; do
    files+=("${best[$name]}")
    found=true
  done
}
if [[ -n $requests ]]; then
  while IFS= read -r pattern; do
    found=false
    # Kernel metadata can contain patterns such as ath11k/*.
    for file in $pattern; do
      if [[ -f $file ]]; then
        files+=("$file")
        found=true
      elif [[ -d $file ]]; then
        matches=$(find -L "$file" -type f)
        if [[ -n $matches ]]; then
          mapfile -t nested <<< "$matches"
          files+=("${nested[@]}")
          found=true
        fi
      fi
    done
    if ! $found && [[ $pattern =~ ^(.*-)([a-z]?)([0-9]+)(\.[a-z]+)$ ]]; then
      prefix=${BASH_REMATCH[1]} tag=${BASH_REMATCH[2]} wanted=${BASH_REMATCH[3]} ext=${BASH_REMATCH[4]}
      # Drivers such as iwlwifi name only their newest firmware API (iwlwifi-ty-a0-gf-a0-100.ucode)
      # and step down until a file loads: keep the newest older version linux-firmware has.
      newest_older "$prefix" "$tag" "$wanted" "$ext"
      # iwlwifi also picks the silicon step at run time: modinfo names bz-a0, linux-firmware ships
      # bz-b0. Try any step (-a0- becomes -??-) for each name linux-firmware has.
      if ! $found; then
        newest_older "$(sed -E 's/-[a-z][0-9](-|$)/-??\1/g' <<< "$prefix")" "$tag" "$wanted" "$ext"
      fi
      # A core-numbered request (c99) with only newer core files shipped: the plain-numbered
      # files of the same names are the older format, so keep their newest too.
      if [[ -n $tag ]]; then
        newest_older "$(sed -E 's/-[a-z][0-9](-|$)/-??\1/g' <<< "$prefix")" "" "$plain_max" "$ext"
      fi
    fi
    if ! $found; then
      printf 'warning: linux-firmware has no match for %s (%s)\n' "$pattern" "$driver" >&2
    fi
  done <<< "$requests"
else
  # A driver with no declared firmware uses its directory or filename prefix.
  # shellcheck disable=SC2206
  paths=( $fallback )
  if (( ${#paths[@]} )); then
    matches=$(find -L "${paths[@]}" -type f)
    if [[ -n $matches ]]; then
      mapfile -t files <<< "$matches"
    fi
  fi
fi

# Drivers with a folder per chip (ath10k/QCA9377/hw1.0) choose files in it at run time, such as
# board-2.bin where modinfo names board.bin, so a chip folder that any request names comes whole.
# ALSO adds chip folders the driver supports but modinfo doesn't name (ath11k/QCA2066).
dirs=()
for file in "${files[@]}"; do
  [[ $file == */*/* ]] && dirs+=("${file%/*}")
done
# ALSO is a space-separated list of globs; IFS is empty, so each word is only globbed.
IFS=' ' read -ra words <<< "$also"
for word in "${words[@]}"; do
  for dir in $word; do
    dirs+=("$dir")
  done
done
for dir in "${dirs[@]}"; do
  while IFS= read -r -d '' file; do
    files+=("$file")
  done < <(find -L "$dir" -type f -print0)
done

if (( ! ${#files[@]} )); then
  printf 'no firmware found for %s\n' "$driver" >&2
  exit 1
fi
printf '%s\n' "${files[@]}" | sort -u
