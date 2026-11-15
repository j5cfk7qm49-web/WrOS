{ pkgs, lib, version, src, rustPlatform, rustToolchain, paths }:
let
  rustSrc = lib.fileset.toSource {
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

  commonNativeBuildInputs = [
    pkgs.pkg-config
    pkgs.patchelf
  ];

  commonBuildInputs = [
    pkgs.alsa-lib
    pkgs.openssl
    pkgs.libdrm
    pkgs.libinput
    pkgs.systemd
    pkgs.seatd
    pkgs.libxkbcommon
    pkgs.wayland
    pkgs.mesa
    pkgs.libGL
  ];

  wgpuRuntimeLibs = lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.libGL
    pkgs.mesa
    pkgs.vulkan-loader
  ];

  mkWrosRustPkg = { pname, cargoBuildFlags, buildFeatures ? [], pathEnv, postFixup ? "", doCheck ? false }:
    rustPlatform.buildRustPackage ({
      inherit pname version doCheck postFixup;
      src = rustSrc;
      cargoLock = {
        lockFile = src + "/Cargo.lock";
      };
      nativeBuildInputs = commonNativeBuildInputs;
      buildInputs = commonBuildInputs;
      buildNoDefaultFeatures = buildFeatures != [];
      inherit buildFeatures cargoBuildFlags;
    } // pathEnv);

  # Release features from docs/RELEASE_CHECKLIST.md: gui-drm,gui-wgpu,audio (never test-driver).
  # Add DT_RUNPATH for dlopen-loaded Wayland/EGL/xkbcommon/GL/Vulkan libraries (H5) without wrapping
  # the binary (so /proc/<pid>/exe remains grimoire for wros-lock and wros-install-session).
  grimoire = mkWrosRustPkg {
    pname = "grimoire";
    buildFeatures = [ "gui-drm" "gui-wgpu" "audio" ];
    cargoBuildFlags = [ "-p" "grimoire-tui" "--bin" "grimoire" ];
    pathEnv = paths.rustPathEnv;
    postFixup = ''
      patchelf --add-rpath "${wgpuRuntimeLibs}" "$out/bin/grimoire"
    '';
  };

  wros-greeter = mkWrosRustPkg {
    pname = "wros-greeter";
    cargoBuildFlags = [ "-p" "wros-greeter" ];
    pathEnv = paths.greeterPathEnv;
  };

  wros-boot = mkWrosRustPkg {
    pname = "wros-boot";
    cargoBuildFlags = [ "-p" "wros-boot" ];
    pathEnv = paths.bootPathEnv;
  };

  wros-installer = mkWrosRustPkg {
    pname = "wros-installer";
    cargoBuildFlags = [ "-p" "wros-installer" ];
    pathEnv = paths.installerPathEnv;
  };

  rust-suite = rustPlatform.buildRustPackage {
    pname = "wros-rust-suite";
    inherit version;
    src = rustSrc;
    cargoLock = {
      lockFile = src + "/Cargo.lock";
    };
    nativeBuildInputs = commonNativeBuildInputs ++ [ rustToolchain ];
    buildInputs = commonBuildInputs;
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      cargo clippy --workspace --all-targets --locked -- -D warnings
      cargo test --workspace --locked
      runHook postCheck
    '';
    installPhase = ''
      mkdir -p "$out"
      echo "pass" > "$out/result"
    '';
  };
in
{
  inherit grimoire wros-greeter wros-boot wros-installer rust-suite;
}
