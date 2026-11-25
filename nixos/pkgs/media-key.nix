{ pkgs, lib, src, rustPlatform }:
# The brightness, volume and music keys' handler (#174, rc30): a small Rust program that triggerhappy
# runs once per press, as the wros-keys account. It replaced os/scripts/wros-media-key, whose
# Python start-up (twice for a volume press, through wros-sound) made the keys feel slow on the
# HP Stream. Built before paths.nix, which names it, so it needs nothing from there.
rustPlatform.buildRustPackage {
  pname = "wros-media-key";
  version = (lib.importTOML (src + "/crates/wros-media-key/Cargo.toml")).package.version;
  src = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/Cargo.toml")
      (src + "/Cargo.lock")
      (src + "/src")
      (src + "/crates")
      (src + "/assets")
      (src + "/tests")
    ];
  };
  cargoLock.lockFile = src + "/Cargo.lock";
  cargoBuildFlags = [ "-p" "wros-media-key" ];
  cargoTestFlags = [ "-p" "wros-media-key" ];
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.alsa-lib ];
  WROS_PATH_BRIGHTNESSCTL = "${pkgs.brightnessctl}/bin/brightnessctl";
  doCheck = true;
}
