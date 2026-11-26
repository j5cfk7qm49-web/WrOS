{ pkgs, lib, version, src, rustPlatform, paths, wros-cage, wros-prose-runtime }:
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

  # The programs the Python screen tests drive: debug builds of the app (without audio, with the
  # pty-qa hook the lock test needs) and of the installer. Never shipped: pty-qa is test-only.
  app-test-binary = rustPlatform.buildRustPackage ({
    pname = "grimoire-test-binary";
    inherit version;
    src = rustSrc;
    cargoLock.lockFile = src + "/Cargo.lock";
    buildType = "debug";
    buildNoDefaultFeatures = true;
    buildFeatures = [ "grimoire-tui/pty-qa" ];
    cargoBuildFlags = [ "-p" "grimoire-tui" "--bin" "grimoire" "-p" "wros-installer" "--bin" "wros-installer" ];
    doCheck = false;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.alsa-lib pkgs.openssl pkgs.libdrm pkgs.libinput pkgs.systemd pkgs.seatd pkgs.libxkbcommon pkgs.wayland pkgs.mesa pkgs.libGL ];
  } // paths.rustPathEnv // {
    WROS_TYPST = lib.getExe pkgs.typst;
    WROS_MLA_FONT_DIR = "${pkgs.liberation_ttf}/share/fonts/truetype";
  });

  wlroots = import ./wlroots-0.18.nix { inherit pkgs; };
  python = pkgs.python3.withPackages (ps: [ ps.pillow ps.qrcode ]);
in
{
  inherit app-test-binary;

  # Every Python test in os/tests except the Debian-only ones tools/nix-python-suite.py lists.
  # runCommandCC: the display-bridge, console-reset and Cage gamma tests compile small C programs.
  app-suite = pkgs.runCommandCC "wros-app-suite" {
    buildInputs = [ pkgs.libdrm pkgs.wayland wlroots pkgs.pixman pkgs.libxkbcommon ];
    nativeBuildInputs = [
      python
      pkgs.git
      pkgs.gnupg
      pkgs.restic
      pkgs.rclone
      pkgs.e2fsprogs
      pkgs.util-linux
      pkgs.procps
      pkgs.which
      pkgs.kbd
      pkgs.console-setup
      pkgs.xkeyboard_config
      pkgs.pkg-config
      pkgs.wayland
      pkgs.wayland-scanner
      wlroots
      wros-cage
      pkgs.foot
      pkgs.libreoffice
      pkgs.poppler-utils
      pkgs.jre_headless
      pkgs.epubcheck
      pkgs.dpkg
    ];
  } ''
    cp -r ${src} source
    chmod -R u+w source
    cd source
    export HOME="$TMPDIR/home" TMPDIR="$TMP"
    mkdir -p "$HOME"
    patchShebangs --build os/scripts nixos/scripts tools os/tests >/dev/null
    # Debian tool paths in the source, given the store's as helpers.nix does for the installed copies.
    sed -i -E 's#/usr/bin/gpgv#${pkgs.gnupg}/bin/gpgv#g; s#/usr/bin/gpg([^v])#${pkgs.gnupg}/bin/gpg\1#g' \
      os/scripts/wros-update-engine.py tools/publish-updates.py tools/build-update-bundle.py \
      os/tests/test_update_workflow.py os/tests/test_update_support.py
    # The Debian channel's publisher (still run by release-updates.yml) builds and reads .debs.
    sed -i -E 's#/usr/bin/(dpkg[a-z-]*)#${pkgs.dpkg}/bin/\1#g' \
      os/scripts/wros-update-engine.py tools/build-update-bundle.py os/tests/test_update_support.py
    sed -i -e 's#/usr/bin/rclone#${pkgs.rclone}/bin/rclone#g' -e 's#/usr/bin/restic#${pkgs.restic}/bin/restic#g' \
      -e 's#/usr/bin/python3#${python}/bin/python3#g' \
      os/scripts/wros-backup os/scripts/wros-sync os/scripts/wros-ytm os/scripts/wros-link.py
    # After the rewrites: some tests read the tree through git (ls-files, show).
    git init -q && git add -A
    git -c user.name=t -c user.email=t@t.invalid commit -qm source
    git config --global user.name "WrOS test"
    git config --global user.email "test@wros.invalid"
    # Foot needs a font configuration to draw; LibreOffice needs the metric-compatible Liberation
    # fonts (as Debian and NixOS provide) for the manuscript layout the footnote checks measure.
    export FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts pkgs.liberation_ttf ]; }}
    # ckbcomp finds XKB under /usr/share/X11/xkb on Debian; here it is told (as keymaps.nix does).
    mkdir -p "$TMPDIR/wros-ckbcomp"
    printf '#!%s\nexec %s -I%s "$@"\n' \
      "${pkgs.runtimeShell}" \
      "${pkgs.console-setup}/bin/ckbcomp" \
      "${pkgs.xkeyboard_config}/share/X11/xkb" > "$TMPDIR/wros-ckbcomp/ckbcomp"
    chmod +x "$TMPDIR/wros-ckbcomp/ckbcomp"
    export PATH="$TMPDIR/wros-ckbcomp:$PATH"
    export GRIMOIRE_BINARY=${app-test-binary}/bin/grimoire
    export WROS_HEADLESS_GRIMOIRE=$GRIMOIRE_BINARY
    export WROS_CAPTURE_PTY=1 WROS_QA_PTY=1
    export WROS_TOOLS_PYTHON=${wros-prose-runtime}/bin/python3
    export WROS_WORDNET_DIR=${pkgs.wordnet}/dict
    export WROS_CAGE=${wros-cage}/bin/cage
    export WROS_INSTALLER_BINARY=${app-test-binary}/bin/wros-installer
    export WROS_EPUBCHECK=${pkgs.epubcheck}/bin/epubcheck
    # As the Rust suites (rust-packages.nix): flushes to disk do nothing during the tests.
    export LD_PRELOAD="${pkgs.libeatmydata}/lib/libeatmydata.so"
    python3 tools/nix-python-suite.py
    mkdir -p "$out"
    echo "pass" > "$out/result"
  '';
}
