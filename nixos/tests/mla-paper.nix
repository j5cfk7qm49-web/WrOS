{ pkgs, wrosPackages }:

pkgs.testers.runNixOSTest {
  name = "wros-mla-paper";

  nodes.writer = { ... }: {
    users.users.writer.isNormalUser = true;
    environment.systemPackages = [
      wrosPackages.grimoire
      pkgs.poppler-utils
      pkgs.python3
    ];
    virtualisation.memorySize = 4096;
  };

  testScript = ''
    writer.start()
    writer.wait_for_unit("multi-user.target")
    result = writer.succeed(
        "runuser -u writer -- env GRIMOIRE_BINARY=/run/current-system/sw/bin/grimoire "
        "PATH=/run/current-system/sw/bin python3 ${../../os/tests/test_mla_export.py} -v"
    )
    print(result)
  '';
}
