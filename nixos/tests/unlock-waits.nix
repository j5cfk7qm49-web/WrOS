{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-unlock-waits";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode { };
  };

  # The disk unlock screen waits for the writer however long they take: leave it untouched past
  # the 5 minutes systemd waited for the root device by default, then unlock, and the system
  # must still start. Before the fix, the wait ended at 5 minutes and the prompt took nothing.
  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    disk_img = build_encrypted_disk(
        installer,
        keyboard_layout="qwerty",
        luks_passphrase="patient-passphrase",
        username="writer",
        user_password="writer-secret-password",
        hostname="wros-book",
    )
    prepare_installed_disk(installed, disk_img)

    installed.start(allow_reboot=True)
    wait_for_disk_prompt(installed)
    time.sleep(330)  # host-side: nobody touches the prompt
    unlock_at_prompt(installed, "patient-passphrase")
    installed.wait_for_unit("multi-user.target")
    installed.wait_for_unit("greetd.service")
  '';
}
