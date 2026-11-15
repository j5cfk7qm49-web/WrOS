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

    CORE = "/run/current-system/sw/libexec/wros-install-core"

    def run_core(payload):
        live.succeed(f"printf '%s' '{json.dumps(payload)}' > /tmp/payload.json")
        return live.execute(f"{CORE} < /tmp/payload.json 2>&1")

    def start_bytes():
        return live.succeed("head -c 4194304 /dev/vdb | sha256sum").split()[0]

    # 1a. Wrong disk (issue #3): the session chose a disk, then a different one appeared under
    #     the same name (new diskseq). Nothing is written.
    blank = start_bytes()
    wrong = {"target_disk": "vdb", "username": "writer", "luks_passphrase": "colemak-passphrase",
             "user_password": "writer-secret-password", "session_nonce": "0" * 32,
             "target_identity": {**disk_identity(live), "diskseq": "999"}}
    status, out = run_core(wrong)
    assert status != 0 and "not the disk chosen" in out, (status, out)
    assert start_bytes() == blank, "the wrong disk was written to"

    # 1b. A disk in use: a filesystem on it is mounted. Refused; the filesystem is untouched.
    live.succeed("mkfs.ext4 -q -L KEEPME /dev/vdb && mkdir -p /mnt/keep && mount /dev/vdb /mnt/keep")
    live.succeed("echo precious > /mnt/keep/file && sync")
    status, out = run_core({**wrong, "target_identity": disk_identity(live)})
    assert status != 0 and "is mounted at /mnt/keep" in out, (status, out)
    live.succeed("grep -qx precious /mnt/keep/file && umount /mnt/keep")
    live.succeed("test \"$(blkid -s LABEL -o value /dev/vdb)\" = KEEPME")

    # 1c. Power cut part-way through an install. The disk must not be bootable afterwards (the
    #     boot loader is written last), and a second install over it must work.
    live.succeed(
        "printf '%s' '" + json.dumps({**wrong, "target_identity": disk_identity(live)}) + "' > /tmp/payload.json"
    )
    live.succeed(f"systemd-run --unit=wros-test-install -p StandardInput=file:/tmp/payload.json {CORE}")
    # The ESP is mounted on the target just before nixos-install starts copying the system.
    live.wait_until_succeeds("findmnt /run/wros-installer/target/boot", timeout=300)
    live.sleep(10)
    live.succeed("systemctl is-active wros-test-install")
    live.crash()
    live.start()
    live.wait_for_unit("multi-user.target")
    live.wait_for_unit("wros-live-session.service")
    # Whatever reached the disk before the cut, there is no boot loader on it.
    if live.execute("blkid -s TYPE -o value /dev/vdb1")[1].strip() == "vfat":
        live.succeed("mkdir -p /mnt/esp && mount -o ro /dev/vdb1 /mnt/esp")
        live.fail("test -e /mnt/esp/EFI/BOOT/BOOTX64.EFI")
        live.succeed("umount /mnt/esp")

    payload = {
        "target_disk": "vdb",
        "target_identity": disk_identity(live),
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
    live.succeed("test -f /run/wros-installer/target/boot/EFI/BOOT/BOOTX64.EFI")
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

    # A machine made with create_machine() is not stopped by the driver's cleanup; without
    # this the test passes and the build then waits on the running VM forever.
    booted.shutdown()
  '';
}
