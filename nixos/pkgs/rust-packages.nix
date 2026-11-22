{ pkgs, lib, version, src, rustPlatform, rustToolchain, paths }:
let
  defaultVersion = version;
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
    pkgs.linux-pam
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

  # The start screens go into the initrd, which an update re-sends whole (35 MB) whenever a byte
  # in it changes (rc23). So wros-boot is built from only what it uses: its crate and the two it
  # depends on, the vendored crossterm, its two font files, and the other workspace members'
  # manifests (Cargo reads every member's), with empty stand-ins for their sources. Grimoire's
  # code changing, or the release number, no longer rebuilds it.
  otherMembers = [ "grimoire-core" "grimoire-app" "wros-sys" "wros-greeter" "wros-input" "wros-installer" ];
  bootSrc = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions ([
      (src + "/Cargo.toml")
      (src + "/Cargo.lock")
      (src + "/crates/wros-boot")
      (src + "/crates/wros-shell")
      (src + "/crates/wros-graphics")
      (src + "/crates/crossterm-upstream")
      (src + "/assets/fonts/unifont-18.0.01.hex.gz")
      (src + "/assets/fonts/wros-narrow.txt")
    ] ++ map (member: src + "/crates/${member}/Cargo.toml") otherMembers);
  };
  bootStandIns = ''
    mkdir -p src tests
    touch src/main.rs tests/driven.rs tests/colour_process.rs
    ${lib.concatMapStrings (member: ''
      mkdir -p crates/${member}/src
      touch crates/${member}/src/lib.rs
    '') otherMembers}
  '';

  mkWrosRustPkg = { pname, cargoBuildFlags, buildFeatures ? [], pathEnv, postFixup ? "", doCheck ? false,
                    version ? defaultVersion, src ? rustSrc, postPatch ? "" }:
    rustPlatform.buildRustPackage ({
      inherit pname version doCheck postFixup src postPatch;
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
    pathEnv = paths.rustPathEnv // {
      WROS_TYPST = lib.getExe pkgs.typst;
      WROS_MLA_FONT_DIR = "${pkgs.liberation_ttf}/share/fonts/truetype";
    };
    postFixup = ''
      patchelf --add-rpath "${wgpuRuntimeLibs}" "$out/bin/grimoire"
      install -Dm644 assets/csl/README.md "$out/share/doc/grimoire/csl/README.md"
      install -Dm644 assets/csl/modern-language-association.csl "$out/share/doc/grimoire/csl/modern-language-association.csl"
      install -Dm644 assets/csl/locales-en-US.xml "$out/share/doc/grimoire/csl/locales-en-US.xml"
    '';
  };

  wros-greeter = mkWrosRustPkg {
    pname = "wros-greeter";
    cargoBuildFlags = [ "-p" "wros-greeter" ];
    pathEnv = paths.greeterPathEnv;
  };

  wros-boot = mkWrosRustPkg {
    pname = "wros-boot";
    # Its own crate version, not the release: see bootSrc.
    version = (lib.importTOML (src + "/crates/wros-boot/Cargo.toml")).package.version;
    src = bootSrc;
    postPatch = bootStandIns;
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
  # The Debian CI's Rust runs, as Nix checks: the workspace in both audio configurations, and
  # the feature-gated tests that need test-only features.
  mkRustSuite = { pname, commands, extraNativeBuildInputs ? [], pathEnv ? paths.rustPathEnv }: rustPlatform.buildRustPackage ({
    inherit pname;
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
        # The driven OpenGL test's virtual keyboard client.
        (src + "/os/tests/protocols")
        (src + "/os/tests/wayland-keys.c")
      ];
    };
    cargoLock = {
      lockFile = src + "/Cargo.lock";
    };
    # git: grimoire-core's writing-session history is a Git repository, and its tests make real ones.
    # wayland-scanner: the compositor-warmth tests build a small gamma-control server.
    nativeBuildInputs = commonNativeBuildInputs ++ [ rustToolchain pkgs.git pkgs.wayland-scanner ] ++ extraNativeBuildInputs;
    buildInputs = commonBuildInputs;
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      export HOME="$TMPDIR"
      # The pinned prose engine runtime the app itself uses.
      export WROS_TOOLS_PYTHON="${paths.toolPaths.toolsPython}"
      git config --global user.name "WrOS test"
      git config --global user.email "test@wros.invalid"
      ${commands}
      runHook postCheck
    '';
    installPhase = ''
      mkdir -p "$out"
      echo "pass" > "$out/result"
    '';
  } // pathEnv);

  # Every crate's tests run even after one fails, so one failure doesn't hide the rest.
  workspaceCommands = flags: ''
    cargo clippy --workspace --all-targets ${flags} --locked -- -D warnings
    cargo test --workspace ${flags} --locked --no-fail-fast
  '';
  rust-suite = mkRustSuite { pname = "wros-rust-suite"; commands = workspaceCommands ""; };
  rust-suite-no-audio = mkRustSuite {
    pname = "wros-rust-suite-no-audio";
    commands = workspaceCommands "--no-default-features";
  };
  # The app in a terminal and in an OpenGL window under headless Cage (tests/driven.rs), and the
  # lock action's readiness check; both need the test-only test-driver feature.
  # Its Settings snapshots record an app whose update helper is not installed (as in a
  # development build), so that one path stays the uninstalled default.
  rust-driven-suite = let cage = import ./cage.nix { inherit pkgs lib src; }; in mkRustSuite {
    pname = "wros-rust-driven-suite";
    extraNativeBuildInputs = [ cage pkgs.mesa ];
    pathEnv = paths.rustPathEnv // { WROS_PATH_HELPER_UPDATE = "/usr/libexec/wros-update"; };
    commands = ''
      export WROS_CAGE="${cage}/bin/cage"
      # As the shipped grimoire (postFixup above): the colour worker starts with a clean
      # environment, so the dlopened Wayland/GL libraries come from the binary's own search path.
      export RUSTFLAGS="-C link-arg=-Wl,-rpath,${wgpuRuntimeLibs}"
      export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri" __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d"
      export LD_LIBRARY_PATH="${lib.makeLibraryPath [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.mesa pkgs.vulkan-loader ]}"
      export XDG_RUNTIME_DIR="$TMPDIR/xdg"; mkdir -m 700 -p "$XDG_RUNTIME_DIR"
      cargo test --no-default-features --features test-driver,gui-wgpu,pty-qa --test driven --locked
      cargo test --no-default-features --features test-driver --bin grimoire locking_is_not_advertised --locked
    '';
  };
in
{
  inherit grimoire wros-greeter wros-boot wros-installer rust-suite rust-suite-no-audio rust-driven-suite;
}
