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

  # The tests see the same tool paths as the shipped app (rustPathEnv), so they run the Nix
  # python3, WordNet data and so on rather than looking under /usr (#131).
  rust-suite = rustPlatform.buildRustPackage ({
    pname = "wros-rust-suite";
    inherit version;
    # The packages' sources, plus the gamma-control protocol and test server the
    # compositor-warmth tests build (kept out of rustSrc so the shipped packages don't rebuild).
    src = lib.fileset.toSource {
      root = src;
      fileset = lib.fileset.unions [
        (src + "/Cargo.toml")
        (src + "/Cargo.lock")
        (src + "/src")
        (src + "/crates")
        (src + "/assets")
        (src + "/tests")
        (src + "/os/tests/wlr-gamma-control-unstable-v1.xml")
        (src + "/os/tests/wayland-gamma-server.c")
        (src + "/os/tests/gamma-frozen-parent.py")
      ];
    };
    cargoLock = {
      lockFile = src + "/Cargo.lock";
    };
    # git: grimoire-core's writing-session history is a Git repository, and its tests make real ones.
    # wayland-scanner: the compositor-warmth tests build a small gamma-control server.
    nativeBuildInputs = commonNativeBuildInputs ++ [ rustToolchain pkgs.git pkgs.wayland-scanner ];
    buildInputs = commonBuildInputs;
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      export HOME="$TMPDIR"
      # The pinned prose engine runtime the app itself uses.
      export WROS_TOOLS_PYTHON="${paths.toolPaths.toolsPython}"
      git config --global user.name "WrOS test"
      git config --global user.email "test@wros.invalid"
      cargo clippy --workspace --all-targets --locked -- -D warnings
      # Every crate's tests run even after one fails, so one failure doesn't hide the rest.
      cargo test --workspace --locked --no-fail-fast
      runHook postCheck
    '';
    installPhase = ''
      mkdir -p "$out"
      echo "pass" > "$out/result"
    '';
  } // paths.rustPathEnv);
in
{
  inherit grimoire wros-greeter wros-boot wros-installer rust-suite;
}
