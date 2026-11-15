{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
rec {
  # Shared installer VM node that formats /dev/vdb with wros-install-core using the
  # test's installed system closure (which includes the NixOS test backdoor service).
  mkInstallerNode = installedToplevel: { config, pkgs, ... }: {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 4096;
      cores = 2;
      emptyDiskImages = [ 8192 ];
      mountHostNixStore = true;
    };
    environment.etc."wros/installed-system-closure".text = lib.mkForce "${installedToplevel}\n";
  };

  # Shared installed-system VM node that boots via UEFI GRUB + systemd initrd LUKS unlock
  # from the encrypted disk image produced by wros-install-core.
  mkInstalledNode = extraModule: { config, pkgs, ... }: {
    imports = [
      installedModule
      extraModule
    ];
    virtualisation = {
      memorySize = 4096;
      cores = 2;
      diskSize = 8192;
      useBootLoader = true;
      useEFIBoot = true;
      mountHostNixStore = true;
      fileSystems."/" = lib.mkVMOverride {
        device = "/dev/disk/by-label/WROS-ROOT";
        fsType = "ext4";
        options = [ "noatime" ];
      };
      fileSystems."/boot" = lib.mkVMOverride {
        device = "/dev/disk/by-label/WROS-ESP";
        fsType = "vfat";
        options = [ "fmask=0077" "dmask=0077" ];
      };
    };
    boot.initrd.luks.devices."wros_crypt" = lib.mkVMOverride {
      device = "/dev/disk/by-label/WROS-LUKS";
      keyFile = "/crypt-ramfs/passphrase";
      keyFileTimeout = 1;
      allowDiscards = true;
    };
  };

  # Shared Python helpers for testScript:
  # - colemak_keys(text): maps desired characters to the physical QWERTY keys that produce
  #   them when the Colemak console keymap is loaded.
  # - build_encrypted_disk(installer_vm, ...): runs wros-install-core inside the live installer
  #   VM onto /dev/vdb, verifies the LUKS2 + GPT layout, unmounts cleanly, and shuts down.
  # - prepare_installed_disk(installed_vm, disk_img): copies the encrypted disk image into
  #   the target VM's state directory before first boot.
  pythonPreamble = ''
    import json
    import shutil
    from pathlib import Path

    COLEMAK_TO_QWERTY = str.maketrans({
        "d": "g", "e": "k", "f": "e", "g": "t", "i": "l",
        "j": "y", "k": "n", "l": "u", "n": "j", "o": ";",
        "p": "r", "r": "s", "s": "d", "t": "f", "u": "i",
        "y": "o", ";": "p",
        "D": "G", "E": "K", "F": "E", "G": "T", "I": "L",
        "J": "Y", "K": "N", "L": "U", "N": "J", "O": ":",
        "P": "R", "R": "S", "S": "D", "T": "F", "U": "I",
        "Y": "O", ":": "P",
    })

    def colemak_keys(text: str) -> str:
        return text.translate(COLEMAK_TO_QWERTY)

    def build_encrypted_disk(
        installer_vm,
        *,
        keyboard_layout: str = "colemak",
        luks_passphrase: str = "colemak-passphrase",
        username: str = "writer",
        user_password: str = "writer-secret-password",
        hostname: str = "wros-book",
        full_name: str = "Ada Writer",
    ) -> Path:
        installer_vm.start()
        installer_vm.wait_for_unit("multi-user.target")
        installer_vm.wait_for_unit("wros-live-session.service")
        payload = {
            "target_disk": "vdb",
            "username": username,
            "full_name": full_name,
            "hostname": hostname,
            "keyboard_layout": keyboard_layout,
            "timezone": "UTC",
            "luks_passphrase": luks_passphrase,
            "user_password": user_password,
            "session_nonce": "0123456789abcdef0123456789abcdef",
        }
        payload_json = json.dumps(payload)
        proof_raw = installer_vm.succeed(
            f"printf '%s' '{payload_json}' | /run/current-system/sw/libexec/wros-install-core"
        )
        proof = json.loads(proof_raw)
        assert proof["passed"] is True, proof
        assert proof["installed_user"] == username, proof
        assert proof["root_locked"] is True, proof
        assert proof["live_account_absent"] is True, proof
        assert proof["encrypted_root_chain"]["format"] == "LUKS2", proof
        installer_vm.succeed("test -s /run/wros-installer/target/boot/grub/grubenv")
        installer_vm.succeed("test -f /run/wros-installer/target/boot/wros-boot/keyboard.json")
        installer_vm.succeed("umount /run/wros-installer/target/boot")
        installer_vm.succeed("umount /run/wros-installer/target")
        installer_vm.succeed("cryptsetup close wros_crypt")
        installer_vm.succeed("sync")
        installer_vm.shutdown()
        disk_img = installer_vm.state_dir / "empty0.qcow2"
        assert disk_img.is_file(), f"missing installed disk image at {disk_img}"
        return disk_img

    def prepare_installed_disk(target_vm, disk_img: Path) -> None:
        target_vm.state_dir.mkdir(mode=0o700, exist_ok=True)
        for candidate in (f"{target_vm.name}.qcow2", "installed.qcow2", "machine.qcow2", "empty0.qcow2"):
            shutil.copyfile(disk_img, target_vm.state_dir / candidate)
  '';
}
