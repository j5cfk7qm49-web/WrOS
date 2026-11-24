{ pkgs }:
let
  fixture = pkgs.runCommand "wros-brain-backup-fixture" {} ''
    mkdir -p "$out/os/scripts" "$out/os/tests"
    substitute ${../../os/scripts/wros-backup} "$out/os/scripts/wros-backup" \
      --replace-fail "/usr/bin/restic" "${pkgs.restic}/bin/restic"
    cp ${../../os/tests/test_native_brain_backup.py} "$out/os/tests/test_native_brain_backup.py"
  '';
in
pkgs.testers.runNixOSTest {
  name = "wros-brain-backup";
  nodes.machine = {
    virtualisation.memorySize = 4096;
    users.users.writer.isNormalUser = true;
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("runuser -u writer -- ${pkgs.python3}/bin/python3 -B ${fixture}/os/tests/test_native_brain_backup.py -v")
  '';
}
