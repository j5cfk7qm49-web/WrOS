{ pkgs, lib, src }:
pkgs.stdenv.mkDerivation {
  pname = "wros-keymaps";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = src;
    fileset = src + "/os/scripts/generate-keymaps.sh";
  };

  nativeBuildInputs = [
    pkgs.console-setup
    pkgs.kbd
    pkgs.xkeyboard_config
  ];

  buildPhase = ''
    runHook preBuild
    mkdir -p "$out/share/wros/keymaps"
    # Debian's ckbcomp finds XKB under /usr/share/X11/xkb by itself. Here it must be told, and the
    # shared recipe stays untouched, so a wrapper earlier in PATH adds the include directory.
    mkdir -p "$TMPDIR/wros-ckbcomp"
    printf '#!%s\nexec %s -I%s "$@"\n' \
      "${pkgs.runtimeShell}" \
      "${pkgs.console-setup}/bin/ckbcomp" \
      "${pkgs.xkeyboard_config}/share/X11/xkb" > "$TMPDIR/wros-ckbcomp/ckbcomp"
    chmod +x "$TMPDIR/wros-ckbcomp/ckbcomp"
    PATH="$TMPDIR/wros-ckbcomp:$PATH"
    sh os/scripts/generate-keymaps.sh "$out/share/wros/keymaps"
    for map in qwerty colemak colemak-dh-ansi colemak-dh-iso; do
      test -s "$out/share/wros/keymaps/$map.map"
    done
    runHook postBuild
  '';

  dontInstall = true;
}
