{ pkgs, lib, version, src }:
let
  cSrc = lib.fileset.toSource {
    root = src;
    fileset = src + "/os/helpers";
  };

  wros-console-reset = pkgs.stdenv.mkDerivation {
    pname = "wros-console-reset";
    inherit version;
    src = cSrc;
    buildPhase = ''
      runHook preBuild
      $CC -O2 -Wall -Wextra -Werror -D_FORTIFY_SOURCE=3 -fstack-protector-strong \
        -Wl,-z,relro,-z,now os/helpers/wros-console-reset.c -o wros-console-reset
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 wros-console-reset "$out/libexec/wros-console-reset"
      install -Dm755 wros-console-reset "$out/bin/wros-console-reset"
      runHook postInstall
    '';
  };

  wros-display-hw = pkgs.stdenv.mkDerivation {
    pname = "wros-display-hw";
    inherit version;
    src = cSrc;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.libdrm ];
    buildPhase = ''
      runHook preBuild
      $CC -O2 -Wall -Wextra -Werror -D_FORTIFY_SOURCE=3 -fstack-protector-strong \
        $(pkg-config --cflags libdrm) os/helpers/wros-display-hw.c \
        $(pkg-config --libs libdrm) -Wl,-z,relro,-z,now -o wros-display-hw
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 wros-display-hw "$out/libexec/wros-display-hw"
      runHook postInstall
    '';
  };
in
{
  inherit wros-console-reset wros-display-hw;
}
