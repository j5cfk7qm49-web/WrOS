{ pkgs, version, wrosPackages }:
pkgs.testers.runNixOSTest {
  name = "wros-media-keys";

  nodes.machine = { ... }: {
    imports = [ ../modules/base.nix ../modules/hardware.nix ];
    _module.args = {
      inherit version;
      wrosPkgs = wrosPackages;
      wrosRoot = ../..;
      nixpkgsRev = "test";
      googleClients = null;
    };
  };

  testScript = ''
    machine.start()
    machine.wait_for_unit("triggerhappy.service")

    log = "/run/wros-keys/music"
    for key, line in [("audioplay", "1 play-pause"), ("audionext", "2 next"), ("audioprev", "3 previous")]:
        machine.send_key(key)
        machine.wait_until_succeeds(f"tail -n 1 {log} | grep -Fx '{line}'", timeout=10)

    assert machine.succeed(f"cat {log}").splitlines() == ["1 play-pause", "2 next", "3 previous"]
    assert machine.succeed(f"stat -c '%U %a' {log}").strip() == "wros-keys 644"
    machine.succeed("test ! -e /run/wros-keys/changed")
  '';
}
