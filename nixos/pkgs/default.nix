{ pkgs, lib, version, src }:
let
  rustToolchain = pkgs.rust-bin.stable."1.100.0".default;
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };

  cHelpers = import ./c-helpers.nix { inherit pkgs lib version src; };
  wros-console-reset = cHelpers.wros-console-reset;
  wros-display-hw = cHelpers.wros-display-hw;

  wros-cage = import ./cage.nix { inherit pkgs lib src; };
  wros-keymaps = import ./keymaps.nix { inherit pkgs lib src; };
  wros-prose-runtime = import ./prose-runtime.nix { inherit pkgs lib src; };
  wros-media-key = import ./media-key.nix { inherit pkgs lib src rustPlatform; };

  wros-helpers = import ./helpers.nix {
    inherit pkgs lib version src wros-display-hw wros-cage wros-keymaps wros-prose-runtime;
  };

  paths = import ./paths.nix {
    inherit pkgs wros-helpers wros-display-hw wros-console-reset wros-cage wros-keymaps wros-prose-runtime
      wros-media-key;
  };

  rustPackages = import ./rust-packages.nix {
    inherit pkgs lib version src rustPlatform rustToolchain paths;
  };

  appTests = import ./app-tests.nix {
    inherit pkgs lib version src rustPlatform paths wros-cage wros-prose-runtime;
  };

  wros-licenses = import ./licenses.nix {
    inherit pkgs lib version src;
  };

  wros-branding = import ./branding.nix { inherit pkgs lib src; };

  hayroBench = pkgs.rustPlatform.buildRustPackage {
    pname = "wros-hayro-bench";
    version = "0.1.0";
    src = src + "/tools/hayro-bench";
    cargoLock.lockFile = src + "/tools/hayro-bench/Cargo.lock";
  };
  hp-measure = import ./hp-measure.nix { inherit pkgs lib src hayroBench; };

  # The tests for what NixOS WrOS runs. The Debian update tests exercise the shared
  # engine's dpkg and apt-get paths and remain excluded by tools/nix-python-suite.py.
  python-suite = pkgs.runCommand "wros-python-suite" {
    nativeBuildInputs = [
      (pkgs.python3.withPackages (ps: [ ps.pillow ]))
      pkgs.gnupg
      pkgs.kmod
      pkgs.nix
      pkgs.util-linux
      pkgs.stdenv.cc
      pkgs.kbd
      pkgs.console-setup
      pkgs.xkeyboard_config
    ];
  } ''
    cp -r ${src} source
    chmod -R u+w source
    cd source
    export TMPDIR="$TMP"
    export PYTHONPATH="os/tests"
    # ckbcomp finds XKB under /usr/share/X11/xkb on Debian; here it is told (as keymaps.nix does).
    mkdir -p "$TMPDIR/wros-ckbcomp"
    printf '#!%s\nexec %s -I%s "$@"\n' \
      "${pkgs.runtimeShell}" \
      "${pkgs.console-setup}/bin/ckbcomp" \
      "${pkgs.xkeyboard_config}/share/X11/xkb" > "$TMPDIR/wros-ckbcomp/ckbcomp"
    chmod +x "$TMPDIR/wros-ckbcomp/ckbcomp"
    export PATH="$TMPDIR/wros-ckbcomp:$PATH"
    # The NixOS engine imports the Debian engine's verification; installed, helpers.nix gives it
    # the store's gpgv. Give the source copy under test the same one.
    substituteInPlace os/scripts/wros-update-engine.py \
      --replace-fail "/usr/bin/gpgv" "${pkgs.gnupg}/bin/gpgv"
    # The rebase kit ships only in Argon handover zips, not in the product tree.
    if [ -f rebase-kit/test_check_rebase.py ]; then python3 rebase-kit/test_check_rebase.py; fi
    python3 -m unittest test_keyboard test_clock test_sound test_media_key test_passwd_helper \
      test_recovery_menu test_boot_settings test_boot_unlock test_session_launcher \
      test_display_manager test_nixos_update test_nixos_install_core test_nixos_installer \
      test_wros_firmware \
      test_update_reply_contract test_update_workflow test_nixos_bundle_tool test_unifont_preview
    mkdir -p "$out"
    echo "pass" > "$out/result"
  '';
in
{
  inherit
    rustToolchain
    rustPlatform
    wros-console-reset
    wros-display-hw
    wros-cage
    wros-keymaps
    wros-prose-runtime
    wros-media-key
    wros-helpers
    wros-licenses
    wros-branding
    hayroBench
    hp-measure
    paths
    python-suite;
  inherit (appTests) app-test-binary app-suite;
  inherit (rustPackages)
    grimoire
    wros-greeter
    wros-boot
    wros-installer
    brain-bench
    rust-suite
    rust-suite-no-audio
    rust-driven-suite;
}
