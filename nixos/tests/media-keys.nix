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
    import time
    machine.start()
    machine.wait_for_unit("triggerhappy.service")
    # At boot udev hands thd devices its own glob already opened (#171). Do it again on
    # purpose: each device must still be open once, and each press must count once.
    machine.succeed("udevadm trigger --action=add --subsystem-match=input && udevadm settle")
    time.sleep(2)
    open_twice = "pid=$(systemctl show -p MainPID --value triggerhappy); ls -l /proc/$pid/fd | grep -o '/dev/input/event[0-9]*' | sort | uniq -d"
    assert machine.succeed(open_twice).strip() == "", "a device is open twice in thd"

    press_log = "/run/wros-keys/music"
    for key, line in [("audioplay", "1 play-pause"), ("audionext", "2 next"), ("audioprev", "3 previous")]:
        machine.send_key(key)
        machine.wait_until_succeeds(f"tail -n 1 {press_log} | grep -Fx '{line}'", timeout=10)

    assert machine.succeed(f"cat {press_log}").splitlines() == ["1 play-pause", "2 next", "3 previous"]
    assert machine.succeed(f"stat -c '%U %a' {press_log}").strip() == "wros-keys 644"
    machine.succeed("test ! -e /run/wros-keys/changed")
  '';
}
