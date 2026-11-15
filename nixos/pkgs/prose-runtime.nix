{ pkgs, lib, src }:
let
  proseSrc = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/docs/python-tools-lock.json")
      (src + "/tools/prepare-prose-tools.py")
      (src + "/os/python-tools")
    ];
  };

  lock = builtins.fromJSON (builtins.readFile (src + "/docs/python-tools-lock.json"));

  fetchedArtifacts = builtins.map (pkg:
    if pkg.source_repack != null then
      pkgs.fetchurl {
        name = pkg.source_repack.archive_filename;
        url = pkg.source_repack.url;
        sha256 = pkg.source_repack.sha256;
      }
    else
      pkgs.fetchurl {
        name = pkg.filename;
        url = pkg.url;
        sha256 = pkg.sha256;
      }
  ) lock.packages;

  # tools/prepare-prose-tools.py refuses symlinked artifacts, so the cache holds real copies.
  artifactCache = pkgs.runCommand "wros-prose-artifacts" { } ''
    mkdir -p "$out"
    ${lib.concatMapStrings (drv: ''
      cp ${drv} "$out/${drv.name}"
    '') fetchedArtifacts}
  '';
in
pkgs.stdenv.mkDerivation {
  pname = "wros-prose-runtime";
  version = "3.8.16-26.7.10-0.16.0";
  src = proseSrc;

  nativeBuildInputs = [
    pkgs.python3
    pkgs.autoPatchelfHook
  ];

  buildInputs = [
    pkgs.stdenv.cc.cc.lib
    pkgs.zlib
  ];

  buildPhase = ''
    runHook preBuild
    mkdir -p work
    python3 tools/prepare-prose-tools.py \
      --offline \
      --cache-dir "${artifactCache}" \
      --python "${pkgs.python3}/bin/python3" \
      --output work
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    cp -r work/runtime "$out"
    chmod -R u+w "$out"
    rm -f "$out/bin/python3" "$out/bin/python" "$out/bin/python3.13"
    ln -s "${pkgs.python3}/bin/python3" "$out/bin/python3"
    ln -s python3 "$out/bin/python"
    ln -s python3 "$out/bin/python3.13"
    cat > "$out/pyvenv.cfg" <<EOF
home = ${pkgs.python3}/bin
include-system-site-packages = false
version = ${pkgs.python3.version}
executable = ${pkgs.python3}/bin/python3
EOF
    runHook postInstall
  '';
}
