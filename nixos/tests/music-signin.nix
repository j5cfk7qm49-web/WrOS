{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
# Spotify sign-in window (#122): the phone reaches librespot's Connect port only while
# wros-music-signin has it open, and the window closes by itself.
pkgs.testers.runNixOSTest {
  name = "wros-music-signin";

  nodes = {
    laptop = { ... }: {
      imports = [ ../modules/music.nix ];
      environment.systemPackages = [ wrosPackages.wros-helpers pkgs.ipset ];
      users.users.writer = { isNormalUser = true; uid = 1000; };
    };
    phone = { pkgs, ... }: {
      environment.systemPackages = [ pkgs.curl ];
    };
  };

  testScript = ''
    import json

    start_all()
    laptop.wait_for_unit("firewall.service")
    phone.wait_for_unit("multi-user.target")

    HELPER = "${wrosPackages.wros-helpers}/libexec/wros-music-signin"
    GETINFO = "curl -sf --max-time 5 'http://laptop:57621/?action=getInfo'"

    # The set and its rules are in place, and empty.
    laptop.succeed("ipset list wros-signin | grep -q 'Type: bitmap:port'")
    rules = laptop.succeed("iptables -S nixos-fw")
    assert "--match-set wros-signin dst" in rules and "192.168.0.0/16" in rules, rules
    laptop.succeed("ip6tables -S nixos-fw | grep -q 'match-set wros-signin dst'")

    # librespot as the writer, the way the app runs it.
    laptop.succeed(
        "install -d -m 700 -o writer /run/user-signin && "
        "su writer -c 'librespot --name WrOS --zeroconf-port 57621 --system-cache /run/user-signin "
        "--cache /run/user-signin --disable-audio-cache --backend pipe --device /dev/null "
        ">/tmp/librespot.log 2>&1 &'"
    )
    laptop.wait_until_succeeds("curl -sf --max-time 2 'http://127.0.0.1:57621/?action=getInfo'")

    with subtest("closed: the phone can't reach the speaker"):
        phone.fail(GETINFO)

    with subtest("open: the phone finds a Spotify Connect speaker named WrOS"):
        laptop.succeed(f"{HELPER} open")
        listing = laptop.succeed("ipset list wros-signin")
        assert "57621 timeout" in listing and "5353 timeout" in listing, listing
        # Members are "<port> timeout <seconds left>".
        members = [line.split() for line in listing.split("Members:")[1].splitlines() if line.strip()]
        assert sorted(m[0] for m in members) == ["5353", "57621"], listing
        assert all(m[1] == "timeout" and 0 < int(m[2]) <= 180 for m in members), listing
        info = json.loads(phone.succeed(GETINFO))
        assert info["deviceType"] == "Speaker" and info["remoteName"] == "WrOS", info

    with subtest("close: blocked again"):
        laptop.succeed(f"{HELPER} close")
        phone.fail(GETINFO)
        laptop.succeed(f"{HELPER} close")  # closing twice is fine

    with subtest("the window closes by itself"):
        laptop.succeed(f"{HELPER} open")
        phone.succeed(GETINFO)
        # Shorten the wait: the same set, a 3-second entry.
        laptop.succeed("ipset -exist add wros-signin 57621 timeout 3")
        phone.wait_until_fails(GETINFO, timeout=20)

    with subtest("the helper takes nothing from its caller"):
        laptop.fail(f"{HELPER} open 22")
        laptop.fail(f"{HELPER} anything")
        laptop.fail(f"su writer -c '{HELPER} open'")  # needs root (pkexec)
  '';
}
