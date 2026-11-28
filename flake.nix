{
  description = "WrOS rc25: offline-first writing appliance on NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/0dc498bfe49baeb18b0f9891e800b53bb2603193";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/8771c4aba509d6efb8bf376fd8cb657cce52b013";
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

      # The installed system at a given version (the release's, or another for the checks below).
      mkWrosSystem = version:
        nixpkgs.lib.nixosSystem {
          inherit system;
          specialArgs = {
            inherit version googleClients;
            wrosPkgs = import ./nixos/pkgs {
              inherit pkgs lib version;
              src = ./.;
            };
            nixpkgsRev = nixpkgs.rev;
            wrosRoot = ./.;  # a path: wrosRoot + "/x" copies only that file
          };
          modules = [
            { nixpkgs.pkgs = pkgs; }
            ./nixos/modules/installed.nix
          ];
        };
      wrosSystem = mkWrosSystem version;

      # rc23: the system must not carry the WrOS source tree. `"${wrosRoot}/x"` and
      # `builtins.readFile "${wrosRoot}/x"` copy and reference the whole repository (44 MB, re-sent
      # in every update); `wrosRoot + "/x"` copies only that file.
      noSourceInClosure = pkgs.runCommand "wros-no-source-in-closure" {
        installed = pkgs.closureInfo { rootPaths = [ wrosSystem.config.system.build.toplevel ]; };
      } ''
        if grep -qxF ${self} "$installed/store-paths"; then
          echo "the installed system carries the WrOS source tree ${self}"
          exit 1
        fi
        touch $out
      '';

      # rc23: the initrd (35 MB, re-sent whole whenever a byte in it changes) is the same for any
      # WrOS version, so an update carries it only when boot code changed.
      initrdOf = v: builtins.unsafeDiscardStringContext
        (mkWrosSystem v).config.system.build.initialRamdisk.drvPath;
      initrdStable = let a = initrdOf version; b = initrdOf "0.1.0~rc99"; in
        pkgs.runCommand "wros-initrd-stable" { } (if a == b then "touch $out" else ''
          echo "the initrd changes with the version string: ${a} vs ${b}"
          exit 1
        '');

      wrosLiveSystem = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = {
          inherit wrosPkgs version googleClients;
          nixpkgsRev = nixpkgs.rev;
          wrosRoot = ./.;  # a path: wrosRoot + "/x" copies only that file
          installedSystemToplevel = wrosSystem.config.system.build.toplevel;
        };
        modules = [
          { nixpkgs.pkgs = pkgs; }
          ./nixos/modules/live.nix
        ];
      };

      wrosIso = wrosLiveSystem.config.system.build.isoImage;

      firmwareTable = pkgs.runCommand "wros-firmware-table-check" {
        nativeBuildInputs = [ pkgs.jq pkgs.nix pkgs.zstd ];
        installedSystem = wrosSystem.config.system.build.toplevel;
        installedFirmware = wrosSystem.config.hardware.firmware;
        installedClosure = pkgs.closureInfo { rootPaths = [ wrosSystem.config.system.build.toplevel ]; };
        # The ISO discards store references after packing this closure into its squashfs.
        liveClosure = pkgs.closureInfo { rootPaths = wrosLiveSystem.config.isoImage.storeContents; };
        expectedPaths = builtins.toJSON (lib.mapAttrs (_: toString) wrosSystem.config.wros.firmware);
      } ''
        set -o pipefail
        table="$installedSystem/wros-firmware.json"
        jq -e --argjson paths "$expectedPaths" '
          .format == 1 and
          (.packages | with_entries(.value = .value.store_path)) == $paths and
          (.packages | all(.[];
            (.store_path | test("^/nix/store/[0-9a-df-np-sv-z]{32}-[^/]+$")) and
            (.nar_sha256 | test("^[0-9a-f]{64}$")) and
            (.nar_size | type == "number" and . > 0 and . == floor) and
            (.modules | type == "array" and all(.[]; type == "string")) and
            (.initrd | type == "boolean")
          )) and
          (.packages | map_values(select(.initrd)) | keys) == ["amdgpu", "i915", "i915-glk", "radeon", "xe"] and
          (.packages.rtw88.modules | index("rtw88_8822be") != null) and
          (.packages["bt-intel"].modules | index("btintel") != null and index("btusb") != null) and
          (.packages.sof.modules | index("snd_sof_pci_intel_apl") != null)
        ' "$table" > /dev/null

        printf 'Package\tNAR bytes\tzstd -19 bytes\n' | tee "$out"
        while IFS=$'\t' read -r driver path hash size; do
          case "$driver" in
            rtw88|i915-glk|sof)
              grep -qxF "$path" "$installedClosure/store-paths" || {
                echo "Installed closure is missing the $driver seed" >&2
                exit 1
              }
              while IFS= read -r -d "" file; do
                if [[ ! -f "$installedFirmware/lib/firmware/$file" ]]; then
                  echo "Installed firmware path is missing $driver seed file $file" >&2
                  exit 1
                fi
              done < <(find -L "$path/lib/firmware" -type f -printf '%P\0')
              ;;
            *)
              if grep -qxF "$path" "$installedClosure/store-paths"; then
                echo "Installed closure includes non-seed firmware $driver" >&2
                exit 1
              fi
              ;;
          esac
          grep -qxF "$path" "$liveClosure/store-paths" || {
            echo "Live ISO closure is missing $driver" >&2
            exit 1
          }
          compressed=$(nix-store --dump "$path" | tee firmware.nar | zstd -19 -c | wc -c)
          if [[ $(sha256sum firmware.nar | cut -d' ' -f1) != "$hash" || $(stat -c %s firmware.nar) != "$size" ]]; then
            echo "Table has the wrong NAR hash or size for $driver" >&2
            exit 1
          fi
          if (( compressed >= 95000000 )); then
            echo "$driver exceeds the Pages limit: $compressed bytes" >&2
            exit 1
          fi
          printf '%s\t%s\t%s\n' "$driver" "$size" "$compressed" | tee -a "$out"
        done < <(jq -r '.packages | to_entries[] | [.key, .value.store_path, .value.nar_sha256, .value.nar_size] | @tsv' "$table")
      '';

      wrosUpdateBundle = import ./nixos/pkgs/update-bundle.nix {
        inherit pkgs lib version;
        src = ./.;  # a path, not the flake string: lib.fileset.toSource requires one
        systemToplevel = wrosSystem.config.system.build.toplevel;
      };

      # The modules take their WrOS arguments through specialArgs in the two systems above. A VM
      # test node is evaluated by the test driver instead, so the same values are supplied here.
      moduleArgs = {
        inherit wrosPkgs version googleClients;
        nixpkgsRev = nixpkgs.rev;
        wrosRoot = ./.;  # a path: wrosRoot + "/x" copies only that file
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
          brain-bench
          hp-measure
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
        wros-firmware-lists = wrosSystem.config.system.build.wrosFirmwareLists;
        brain-pdf-ocr-bench = pkgs.writeShellApplication {
          name = "brain-pdf-ocr-bench";
          runtimeInputs = [
            pkgs.python3
            pkgs.poppler-utils
            (pkgs.tesseract5.override { enableLanguages = [ "eng" "chi_sim" "chi_tra" ]; })
          ];
          text = ''
            exec python3 -B ${./tools/brain-pdf-ocr-bench.py} run \
              --pdfium "${pkgs.pdfium-binaries}/lib/libpdfium.so" \
              --hayro "${wrosPkgs.hayroBench}/bin/wros-hayro-bench" "$@"
          '';
        };
        default = wrosPkgs.grimoire;
      } // lib.mapAttrs' (driver: pkg: lib.nameValuePair "wros-firmware-${driver}" pkg)
        wrosSystem.config.wros.firmware;

      checks.${system} = {
        firmware-lists-current = wrosSystem.config.system.build.wrosFirmwareListsCurrent;
        firmware-table = firmwareTable;
        firmware-set = vmTest ./nixos/tests/firmware-set.nix;
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
        brain-backup = vmTest ./nixos/tests/brain-backup.nix;
        mla-paper = vmTest ./nixos/tests/mla-paper.nix;
        helpers-no-fhs = vmTest ./nixos/tests/helpers-no-fhs.nix;
        start-frames = vmTest ./nixos/tests/start-frames.nix;
        boot-seams = vmTest ./nixos/tests/boot-seams.nix;
        media-keys = vmTest ./nixos/tests/media-keys.nix;
        no-source-in-closure = noSourceInClosure;
        initrd-stable = initrdStable;
        thin-update = vmTest ./nixos/tests/thin-update.nix;
        console-lock-vlock = vmTest ./nixos/tests/console-lock-vlock.nix;
        console-lock-text = vmTest ./nixos/tests/console-lock-text.nix;
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
