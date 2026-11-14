{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };
in
pkgs.testers.runNixOSTest {
  name = "wros-install-encrypted";

  nodes = {
    live = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };
    installed = encryptedDisk.mkInstalledNode {};
  };

  testScript = { nodes, ... }: ''
    ${encryptedDisk.pythonPreamble}

    # 1. Start the live medium and run wros-install-core onto /dev/vdb
    live.start()
    live.wait_for_unit("multi-user.target")
    live.wait_for_unit("wros-live-session.service")
    live.succeed("test -f /run/wros-live/session.json")
    live.succeed("test -s /etc/wros/installed-system-closure")

    payload = {
        "target_disk": "vdb",
        "username": "writer",
        "full_name": "Ada Writer",
        "hostname": "wros-book",
        "keyboard_layout": "colemak",
        "timezone": "UTC",
        "luks_passphrase": "colemak-passphrase",
        "user_password": "writer-secret-password",
        "session_nonce": "0123456789abcdef0123456789abcdef",
    }
    proof = json.loads(
        live.succeed(
            f"printf '%s' '{json.dumps(payload)}' | /run/current-system/sw/libexec/wros-install-core"
        )
    )
    assert proof["passed"] is True
    assert proof["installed_user"] == "writer"
    assert proof["root_locked"] is True
    assert proof["live_account_absent"] is True
    assert proof["encrypted_root_chain"]["format"] == "LUKS2"

    # 2. Verify GPT + WROS-ESP + WROS-LUKS -> wros_crypt -> WROS-ROOT labels and seeded state
    live.succeed("blkid -L WROS-ESP")
    live.succeed("blkid -L WROS-LUKS")
    live.succeed("blkid -L WROS-ROOT")
    live.succeed("test -s /run/wros-installer/target/boot/grub/grubenv")
    live.succeed("grep -q 'colemak' /run/wros-installer/target/etc/wros/keyboard.json")
    live.succeed("grep -q 'colemak' /run/wros-installer/target/boot/wros-boot/keyboard.json")
    live.succeed("test \"$(cat /run/wros-installer/target/etc/hostname)\" = 'wros-book'")
    live.succeed("grep -q '^writer:x:1000:' /run/wros-installer/target/etc/passwd")
    live.fail("grep -q '^wros-live:' /run/wros-installer/target/etc/passwd")
    live.succeed("grep -q '^root:!' /run/wros-installer/target/etc/shadow")
    live.succeed("grep -q 'wros-greeter' /run/wros-installer/target/etc/greetd/config.toml")
    live.fail("grep -q 'initial_session' /run/wros-installer/target/etc/greetd/config.toml")

    # 3. Unmount target, close wros_crypt, and shut down the live installer VM
    live.succeed("umount /run/wros-installer/target/boot")
    live.succeed("umount /run/wros-installer/target")
    live.succeed("cryptsetup close wros_crypt")
    live.succeed("sync")
    live.shutdown()

    # 4. Boot a fresh machine from the newly installed encrypted disk via create_machine (T2)
    disk_img = live.state_dir / "empty0.qcow2"
    booted = create_machine(
        start_command="${nodes.installed.system.build.vm}/bin/run-installed-vm",
        name="installed",
    )
    prepare_installed_disk(booted, disk_img)
    booted.start()

    # 5. Unlock the encrypted disk at the graphical prompt using the Colemak passphrase
    # The guest shell does not exist until after boot, so the screen cannot be read through it
    # here. Wait for the unlock unit's start line on the serial console, give the prompt time
    # to draw, then type.
    wait_for_disk_prompt(booted)
    booted.send_chars(colemak_keys("colemak-passphrase"))
    booted.send_key("ret")

    # 6. Reach greetd login screen and verify installed system state
    booted.wait_for_unit("multi-user.target")
    booted.wait_for_unit("greetd.service")
    booted.wait_for_unit("wros-boot-ok.service")
    booted.succeed("mount | grep -q '/dev/mapper/wros_crypt on / type ext4'")
    booted.succeed("test \"$(cat /etc/hostname)\" = 'wros-book'")
    booted.fail("id wros-live")
    booted.succeed("passwd -S root | awk '{exit ($2 == \"L\" ? 0 : 1)}'")
    wait_for_greeter(booted)
  '';
}
