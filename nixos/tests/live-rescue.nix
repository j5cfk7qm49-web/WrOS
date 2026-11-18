{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-live-rescue";

  nodes.live = {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 2560;
      cores = 2;
      emptyDiskImages = [ 8192 ];
    };
    # The test driver usually connects once multi-user.target starts.
    specialisation.rescue.configuration.systemd.services.backdoor.wantedBy = [
      "wros-live-rescue.target"
    ];
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    disk_img = build_encrypted_disk(live, keyboard_layout="qwerty")
    rescue = create_machine(
        start_command=(
            "${nodes.live.specialisation.rescue.configuration.system.build.vm}"
            "/bin/run-live-vm"
        ),
        name="rescue",
    )
    shutil.copyfile(disk_img, rescue.state_dir / "empty0.qcow2")

    rescue.start()
    try:
        rescue.wait_for_unit("wros-live-rescue.target")
        rescue.wait_for_unit("wros-live-rescue.service")
        rescue.wait_until_tty_matches("1", "Enter passphrase for /dev/vdb2:")
        assert rescue.succeed("ps -C wros-rescue -o euid=,tty=").split() == ["0", "tty1"]
        rescue.fail(
            "systemctl is-active greetd.service wros-live-session.service getty@tty1.service"
        )
        # The whole repair: unlock, mount, run the installed system's engine in the chroot, and
        # lock the disk again; then the menu offers the next step.
        rescue.send_chars("colemak-passphrase\n")
        rescue.wait_until_tty_matches("1", "Rescue finished", timeout=600)
        rescue.wait_until_tty_matches("1", "Choose:")
        rescue.fail("test -e /dev/mapper/wros_crypt")
        rescue.fail("mountpoint -q /target")
        rescue.shutdown()
    finally:
        rescue.release()
  '';
}
