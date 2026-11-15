{ pkgs, lib, src }:
pkgs.stdenv.mkDerivation {
  pname = "wros-cage";
  version = "0.2.0+wros1";
  src = lib.fileset.toSource {
    root = src + "/os/vendor/cage";
    fileset = src + "/os/vendor/cage";
  };

  nativeBuildInputs = [
    pkgs.meson
    pkgs.ninja
    pkgs.pkg-config
    pkgs.wayland-scanner
  ];

  buildInputs = [
    (import ./wlroots-0.18.nix { inherit pkgs; })
    pkgs.wayland
    pkgs.wayland-protocols
    pkgs.libxkbcommon
    pkgs.pixman
    pkgs.libdrm
    pkgs.mesa
    pkgs.libinput
    pkgs.seatd
    # wlroots' xwayland header includes xcb/xcb.h even though this cage is built without Xwayland.
    pkgs.libxcb
    pkgs.libxcb-wm
  ];

  mesonFlags = [
    "-Dman-pages=disabled"
    "--buildtype=release"
  ];

  postInstall = ''
    mkdir -p "$out/lib/wros"
    ln -s "$out/bin/cage" "$out/lib/wros/cage"
  '';
}
