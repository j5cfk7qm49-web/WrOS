{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };

  generation = name: { config, pkgs, lib, ... }:
    let
      inherit (config.boot.kernelPackages) kernel kernelModuleMakeFlags;
      testFirmware = kernel.stdenv.mkDerivation {
        name = "test-firmware-${kernel.modDirVersion}";
        nativeBuildInputs = kernel.moduleBuildDependencies;
        hardeningDisable = [ "pic" "format" ];
        unpackPhase = ''
          tar -xOf ${kernel.src} --wildcards '*/lib/test_firmware.c' > test_firmware.c
          printf '%s\n' 'obj-m := test_firmware.o' > Makefile
        '';
        buildPhase = ''
          make -C ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build \
            ${lib.escapeShellArgs kernelModuleMakeFlags} M="$PWD" modules
        '';
        installPhase = ''
          install -Dm644 test_firmware.ko "$out/lib/modules/${kernel.modDirVersion}/extra/test_firmware.ko"
        '';
      };
      firmware = phase: pkgs.writeTextFile {
        name = "wros-test-firmware-${phase}-${name}";
        destination = "/lib/firmware/wros-test-${phase}.bin";
        text = "${phase} ${name}\n";
      };
    in
    {
      # Build the test driver against the stock kernel used by WrOS.
      boot.extraModulePackages = [ testFirmware ];
      boot.initrd.kernelModules = [ "test_firmware" ];
      boot.initrd.systemd.services.wros-test-firmware = {
        wantedBy = [ "initrd.target" ];
        before = [ "initrd.target" ];
        after = [ "systemd-modules-load.service" ];
        unitConfig.DefaultDependencies = false;
        serviceConfig.Type = "oneshot";
        script = ''
          test "$(cat /sys/module/firmware_class/parameters/path)" = /wros-firmware
          printf '%s' wros-test-early.bin > /sys/devices/virtual/misc/test_firmware/trigger_request
          test "$(cat /dev/test_firmware)" = 'early ${name}'
          test ! -e /wros-firmware/wros-test-late.bin
          printf '%s\n' 'early ${name}' > /run/wros-test-firmware
        '';
      };

      # The QEMU disk and its PCI transport give detection two different modules.
      # The fixture packages come with the system, as the Stream's seed packages do.
      system.systemBuilderCommands = lib.mkAfter ''
        describe_firmware() {
          local nar="$TMPDIR/wros-test-firmware.nar"
          ${pkgs.nix}/bin/nix-store --dump "$1" > "$nar"
          ${pkgs.jq}/bin/jq -n --arg store_path "$1" --arg module "$2" --argjson initrd "$3" \
            --arg nar_sha256 "$(sha256sum "$nar" | cut -d' ' -f1)" \
            --argjson nar_size "$(stat -c %s "$nar")" \
            '{$store_path, $nar_sha256, $nar_size, modules: [$module], $initrd}'
        }
        ${pkgs.jq}/bin/jq \
          --argjson early "$(describe_firmware ${firmware "early"} virtio_blk true)" \
          --argjson late "$(describe_firmware ${firmware "late"} virtio_pci false)" \
          '.packages += {"wros-test-early": $early, "wros-test-late": $late}' \
          "$out/wros-firmware.json" > "$out/test-table.json"
        mv "$out/test-table.json" "$out/wros-firmware.json"
      '';
    };
in
pkgs.testers.runNixOSTest {
  name = "wros-firmware-set";
  defaults.virtualisation.restrictNetwork = true;

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode (generation "first");
    updated = encryptedDisk.mkInstalledNode (generation "second");
  };

  testScript = { nodes, ... }:
    let
      nextClosure = pkgs.closureInfo { rootPaths = [ nodes.updated.system.build.toplevel ]; };
    in ''
    ${encryptedDisk.pythonPreamble}

    first = "${nodes.installed.system.build.toplevel}"
    second = "${nodes.updated.system.build.toplevel}"
    profile = "/nix/var/nix/profiles/system"
    trigger = "/sys/devices/virtual/misc/test_firmware/trigger_request"
    disk_password = "writer-disk-passphrase"

    disk_img = build_encrypted_disk(installer, keyboard_layout="qwerty", luks_passphrase=disk_password)
    prepare_installed_disk(installed, disk_img)

    def boot_and_unlock():
        installed.start(allow_reboot=True)
        wait_for_disk_prompt(installed)
        unlock_at_prompt(installed, disk_password)
        installed.wait_for_unit("multi-user.target")
        installed.wait_for_unit("wros-boot-ok.service")

    def check_firmware(system, name):
        generation = Path(system).name
        expected = f"/nix/var/wros/firmware/{generation}/lib/firmware"
        assert installed.succeed("readlink -f /run/current-system").strip() == system
        assert installed.succeed("readlink /lib/firmware").strip() == expected
        installed.succeed(f"test -s /boot/kernels/{generation}-secrets")
        assert installed.succeed("cat /run/wros-test-firmware").strip() == f"early {name}"
        installed.succeed(
            f'test "$(cat /sys/module/firmware_class/parameters/path)" = "$(readlink {system}/firmware)"'
        )
        installed.succeed(f"printf %s wros-test-late.bin > {trigger}")
        assert installed.succeed("cat /dev/test_firmware").strip() == f"late {name}"

    with subtest("GRUB boots the first installed firmware set"):
        boot_and_unlock()
        check_firmware(first, "first")

    with subtest("GRUB creates a set when a generation is staged without one"):
        installed.succeed("nix-store --load-db < ${nextClosure}/registration")
        installed.succeed(f"nix-env --option substitute false --profile {profile} --set {second}")
        second_set = f"/nix/var/wros/firmware/{Path(second).name}"
        installed.fail(f"test -e {second_set}")
        installed.succeed(f"{second}/bin/switch-to-configuration boot")
        installed.succeed(f"test -f {second_set}/packages.json")
        installed.shutdown()
        boot_and_unlock()
        check_firmware(second, "second")

    with subtest("Go back restores the first generation's early and late firmware"):
        installed.succeed(f"nix-env --option substitute false --profile {profile} --rollback")
        installed.succeed(f"{first}/bin/switch-to-configuration boot")
        installed.shutdown()
        boot_and_unlock()
        check_firmware(first, "first")

    with subtest("Activation logs a refused link and still boots with regdb"):
        installed.succeed("mv /lib/firmware /lib/firmware.saved && mkdir /lib/firmware")
        installed.shutdown()
        boot_and_unlock()
        installed.succeed("test -d /lib/firmware && test ! -L /lib/firmware")
        installed.succeed(
            f'test "$(cat /sys/module/firmware_class/parameters/path)" = "$(readlink {first}/firmware)"'
        )
        installed.succeed("journalctl -b --grep='wros-firmware.*Expected a trusted symlink' | grep wros-firmware")
        installed.fail(f"printf %s wros-test-late.bin > {trigger}")
        installed.succeed(f"printf %s regulatory.db > {trigger}")
        installed.succeed('test -n "$(od -An -N 1 -tx1 /dev/test_firmware)"')
  '';
}
