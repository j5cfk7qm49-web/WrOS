{
  description = "WrOS rc18: offline-first writing appliance on NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/84edd48f141b0c8910dffdd0296ddd577e829132";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      # One package set for the installed system, the live medium, the VM tests and the packages.
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config.allowUnfreePredicate = pkg:
          builtins.elem (nixpkgs.lib.getName pkg) [
            "linux-firmware"
            "sof-firmware"
            "wireless-regdb"
          ];
      };
      lib = pkgs.lib;
      version = lib.strings.trim (builtins.readFile ./os/VERSION);

      # WrOS's Google clients for phone linking (#140). Kept out of the repository: a release
      # build passes the file with WROS_GOOGLE_CLIENTS=/path and --impure. Any other build
      # (CI, tests) has none, and the app hides the phone QR option.
      googleClientsPath = builtins.getEnv "WROS_GOOGLE_CLIENTS";
      googleClients = if googleClientsPath == "" then null else builtins.readFile googleClientsPath;

      wrosPkgs = import ./nixos/pkgs {
        inherit pkgs lib version;
        src = ./.;  # a path, not the flake string: lib.fileset.toSource requires one
      };

      wrosSystem = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = {
          inherit wrosPkgs version googleClients;
          nixpkgsRev = "f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0";
          wrosRoot = self;
        };
        modules = [
          { nixpkgs.pkgs = pkgs; }
          ./nixos/modules/installed.nix
        ];
      };

      wrosLiveSystem = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = {
          inherit wrosPkgs version googleClients;
          nixpkgsRev = "f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0";
          wrosRoot = self;
          installedSystemToplevel = wrosSystem.config.system.build.toplevel;
        };
        modules = [
          { nixpkgs.pkgs = pkgs; }
          ./nixos/modules/live.nix
        ];
      };

      wrosIso = wrosLiveSystem.config.system.build.isoImage;

      wrosUpdateBundle = import ./nixos/pkgs/update-bundle.nix {
        inherit pkgs lib version;
        src = ./.;  # a path, not the flake string: lib.fileset.toSource requires one
        systemToplevel = wrosSystem.config.system.build.toplevel;
      };

      # The modules take their WrOS arguments through specialArgs in the two systems above. A VM
      # test node is evaluated by the test driver instead, so the same values are supplied here.
      moduleArgs = {
        inherit wrosPkgs version googleClients;
        nixpkgsRev = "f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0";
        wrosRoot = self;
        installedSystemToplevel = wrosSystem.config.system.build.toplevel;
      };
      vmTestArgs = {
        inherit pkgs lib version;
        wrosPackages = wrosPkgs;
        installedModule = { imports = [ ./nixos/modules/installed.nix ]; _module.args = moduleArgs; };
        liveModule = { imports = [ ./nixos/modules/live.nix ]; _module.args = moduleArgs; };
      };
      # Each test file names the arguments it needs; pass exactly those.
      vmTest = file: import file (builtins.intersectAttrs (builtins.functionArgs (import file)) vmTestArgs);
    in
    {
      nixosConfigurations = {
        wros = wrosSystem;
        wros-live = wrosLiveSystem;
      };

      packages.${system} = {
        inherit (wrosPkgs)
          grimoire
          wros-greeter
          wros-boot
          wros-installer
          wros-console-reset
          wros-display-hw
          wros-cage
          wros-keymaps
          wros-prose-runtime
          wros-helpers
          wros-licenses
          wros-branding;
        wros-iso = wrosIso;
        wros-update-bundle = wrosUpdateBundle;
        default = wrosPkgs.grimoire;
      };

      checks.${system} = {
        boot-unlock-login = vmTest ./nixos/tests/boot-unlock-login.nix;
        keyboard-preview = vmTest ./nixos/tests/keyboard-preview.nix;
        console-lock = vmTest ./nixos/tests/console-lock.nix;
        update-goback = vmTest ./nixos/tests/update-goback.nix;
        recovery-menu = vmTest ./nixos/tests/recovery-menu.nix;
        install-encrypted = vmTest ./nixos/tests/install-encrypted.nix;
        live-session = vmTest ./nixos/tests/live-session.nix;
        live-rescue = vmTest ./nixos/tests/live-rescue.nix;
        live-graphical = vmTest ./nixos/tests/live-graphical.nix;
        unlock-waits = vmTest ./nixos/tests/unlock-waits.nix;
        import-book = vmTest ./nixos/tests/import-book.nix;
        book-timeline = vmTest ./nixos/tests/book-timeline.nix;
        helpers-no-fhs = vmTest ./nixos/tests/helpers-no-fhs.nix;
        start-frames = vmTest ./nixos/tests/start-frames.nix;
        boot-seams = vmTest ./nixos/tests/boot-seams.nix;
        rust-suite = wrosPkgs.rust-suite;
        rust-suite-no-audio = wrosPkgs.rust-suite-no-audio;
        rust-driven-suite = wrosPkgs.rust-driven-suite;
        python-suite = wrosPkgs.python-suite;
        app-suite = wrosPkgs.app-suite;
      };

      # `nix develop`: the pinned toolchain, the app's build inputs and the same compiled tool
      # paths as the shipped app, for building and testing from a checkout. CI runs its
      # repository gates here.
      devShells.${system}.default = pkgs.mkShell ({
        packages = [
          wrosPkgs.rustToolchain
          pkgs.pkg-config
          pkgs.git
          pkgs.python3
          pkgs.shellcheck
          pkgs.wayland-scanner
          pkgs.gnupg
          pkgs.kbd
          pkgs.console-setup
          pkgs.xkeyboard_config
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
        WROS_TOOLS_PYTHON = wrosPkgs.paths.toolPaths.toolsPython;
      } // wrosPkgs.paths.rustPathEnv);
    };
}
