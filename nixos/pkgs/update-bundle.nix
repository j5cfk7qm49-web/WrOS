{ pkgs, lib, version, src, systemToplevel, baselineToplevel ? null, firmwarePackages ? { } }:
let
  roots = [ systemToplevel ] ++ lib.attrValues firmwarePackages;
  targetClosure = pkgs.closureInfo { rootPaths = roots; };
  firmwarePaths = builtins.toJSON (lib.mapAttrs (_: toString) firmwarePackages);
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

    # The closure's paths are present in the sandbox's /nix/store, but this private Nix
    # database does not know them, so `nix copy` refused ("no substituter that can build it").
    # Register them first, as nixos/lib/make-disk-image.nix does.
    nix-store --load-db < "${targetClosure}/registration"

    # Populate one local cache for the target closure and every firmware package.
    nix --extra-experimental-features nix-command copy \
      --to "file://$out/cache?compression=zstd" \
      --no-check-sigs \
      ${lib.escapeShellArgs (map toString roots)}
    nix-store -qR "${systemToplevel}" > system-paths

    python3 - \
      "${src + "/tools/build-nixos-update-bundle.py"}" \
      system-paths \
      "${if baselineClosure != null then "${baselineClosure}/store-paths" else ""}" \
      "${systemToplevel}" \
      "${if baselineToplevel != null then "${baselineToplevel}" else ""}" \
      "${version}" \
      ${lib.escapeShellArg firmwarePaths} \
      "$out" << 'EOF'
import json
from pathlib import Path
import runpy
import sys

tool, system_paths, baseline_paths, toplevel, baseline, version, firmware, out = sys.argv[1:]
out = Path(out)
collect = runpy.run_path(tool)['collect_cache_manifest']
manifest = collect(
    out, toplevel, version, set(Path(system_paths).read_text().splitlines()), json.loads(firmware),
    baseline=set(Path(baseline_paths).read_text().splitlines()) if baseline_paths else set(),
    baseline_toplevel=baseline or None,
)

(out / "closure-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
EOF
  '';
}
