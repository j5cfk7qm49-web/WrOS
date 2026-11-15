{ pkgs, lib, version, src, systemToplevel, baselineToplevel ? null }:
let
  targetClosure = pkgs.closureInfo { rootPaths = [ systemToplevel ]; };
  baselineClosure =
    if baselineToplevel != null then
      pkgs.closureInfo { rootPaths = [ baselineToplevel ]; }
    else
      null;
in
pkgs.stdenv.mkDerivation {
  pname = "wros-update-bundle";
  inherit version;

  nativeBuildInputs = [
    pkgs.python3
    pkgs.nix
    pkgs.zstd
  ];

  buildCommand = ''
    mkdir -p "$out/cache"
    export HOME="$TMPDIR/home"
    export NIX_STATE_DIR="$TMPDIR/nix-state"
    mkdir -p "$HOME" "$NIX_STATE_DIR"

    # Populate a local zstd-compressed Nix binary cache for the target closure (U2, U5)
    nix --extra-experimental-features nix-command copy \
      --to "file://$out/cache?compression=zstd" \
      --no-check-sigs \
      "${systemToplevel}"

    python3 - \
      "${targetClosure}/store-paths" \
      "${if baselineClosure != null then "${baselineClosure}/store-paths" else ""}" \
      "${systemToplevel}" \
      "${if baselineToplevel != null then "${baselineToplevel}" else ""}" \
      "${version}" \
      "$out" << 'EOF'
import hashlib
import json
from pathlib import Path
import sys

target_paths_file = Path(sys.argv[1])
baseline_paths_arg = sys.argv[2]
toplevel = sys.argv[3]
baseline_toplevel = sys.argv[4] or None
version = sys.argv[5]
out = Path(sys.argv[6])
cache_dir = out / "cache"

target_paths = {p.strip() for p in target_paths_file.read_text().splitlines() if p.strip()}
baseline_paths = set()
if baseline_paths_arg:
    baseline_paths = {p.strip() for p in Path(baseline_paths_arg).read_text().splitlines() if p.strip()}

# Map each .narinfo in cache/ to its StorePath and referenced URL (nar/*.nar.zst)
narinfo_by_store = {}
for narinfo_path in sorted(cache_dir.glob("*.narinfo")):
    fields = {}
    for line in narinfo_path.read_text(encoding="utf-8").splitlines():
        if ": " in line:
            k, v = line.split(": ", 1)
            fields[k] = v.strip()
    sp = fields.get("StorePath", "")
    url = fields.get("URL", "")
    if sp in baseline_paths and sp != toplevel:
        # Delta bundle (U5): omit store paths already in baseline closure
        narinfo_path.unlink(missing_ok=True)
        if url:
            (cache_dir / url).unlink(missing_ok=True)
        continue
    narinfo_by_store[narinfo_path.name] = (sp, url)

cache_files = []
for file_path in sorted(cache_dir.rglob("*")):
    if not file_path.is_file():
        continue
    rel = file_path.relative_to(out).as_posix()
    data = file_path.read_bytes()
    sha256 = hashlib.sha256(data).hexdigest()
    sp = None
    if file_path.parent == cache_dir and file_path.name in narinfo_by_store:
        sp = narinfo_by_store[file_path.name][0]
    elif file_path.parent.name == "nar":
        nar_rel = f"nar/{file_path.name}"
        for _, (cand_sp, cand_url) in narinfo_by_store.items():
            if cand_url == nar_rel:
                sp = cand_sp
                break
    cache_files.append({
        "path": rel,
        "sha256": sha256,
        "size": len(data),
        "store_path": sp,
    })

manifest = {
    "schema": 2,
    "format": "wros-nixos-cache-v2",
    "version": version,
    "system": "x86_64-linux",
    "toplevel": toplevel,
    "baseline": baseline_toplevel,
    "app_only": False,
    "cache_files": cache_files,
}
(out / "closure-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
EOF
  '';
}
