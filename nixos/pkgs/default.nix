{ pkgs, lib, version, src }:
let
  rustToolchain = pkgs.rust-bin.stable."1.99.1".default;
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

  wros-helpers = import ./helpers.nix {
    inherit pkgs lib version src wros-display-hw wros-cage wros-keymaps wros-prose-runtime;
  };

  paths = import ./paths.nix {
    inherit pkgs wros-helpers wros-display-hw wros-console-reset wros-cage wros-keymaps wros-prose-runtime;
  };

  rustPackages = import ./rust-packages.nix {
    inherit pkgs lib version src rustPlatform rustToolchain paths;
  };

  wros-licenses = import ./licenses.nix {
    inherit pkgs lib version src;
  };

  wros-branding = import ./branding.nix { inherit pkgs lib src; };

  # The tests for what NixOS WrOS runs. The Debian update engine's own suites (test_update,
  # test_update_live, test_update_rollback, test_update_support, test_update_transactions) drive
  # dpkg and apt-get against a Debian archive; they stay in the Debian builder's suite (#131).
  python-suite = pkgs.runCommand "wros-python-suite" {
    nativeBuildInputs = [ pkgs.python3 pkgs.gnupg pkgs.kbd pkgs.console-setup pkgs.xkeyboard_config ];
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
    # The rebase kit ships only in Argon handover zips, not in the product tree.
    if [ -f rebase-kit/test_check_rebase.py ]; then python3 rebase-kit/test_check_rebase.py; fi
    python3 -m unittest test_keyboard test_clock test_sound test_media_key test_passwd_helper \
      test_recovery_menu test_boot_settings test_boot_unlock test_session_launcher \
      test_display_manager test_nixos_update test_nixos_install_core
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
    wros-helpers
    wros-licenses
    wros-branding
    paths
    python-suite;
  inherit (rustPackages)
    grimoire
    wros-greeter
    wros-boot
    wros-installer
    rust-suite;
}
