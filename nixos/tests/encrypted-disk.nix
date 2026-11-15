{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
rec {
  # Shared installer VM node that formats /dev/vdb with wros-install-core using the
  # test's installed system closure (which includes the NixOS test backdoor service).
  mkInstallerNode = installedToplevel: { config, pkgs, ... }: {
    imports = [ liveModule ];
    virtualisation = {
      memorySize = 2560;  # was 4096: a 4 GiB guest starves the 16 GB qualification host
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
      memorySize = 2560;  # was 4096: a 4 GiB guest starves the 16 GB qualification host
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
    # The test driver names every node, which makes NixOS manage /etc/hostname and replace the
    # name the installer wrote. The real system leaves the host name unmanaged; so must this one.
    networking.hostName = lib.mkForce "";
    # The driver finds a machine by its start script, run-<system.name>-vm, and system.name
    # follows the host name. Keep it equal to the node's name.
    system.name = config.virtualisation.test.nodeName;
    # The test driver talks to the guest through backdoor.service, which test-instrumentation.nix
    # starts from multi-user.target. The recovery entry boots wros-recovery.target instead, so
    # start the driver's service there too (test machines only).
    systemd.services.backdoor.wantedBy = [ "wros-recovery.target" ];
    # qemu-vm.nix sets the whole attribute set to {} with mkVMOverride (priority 10). An override
    # on one attribute does not beat that; the node ended up with no encrypted device at all.
    boot.initrd.luks.devices = lib.mkOverride 9 {
      wros_crypt = {
        device = "/dev/disk/by-label/WROS-LUKS";
        keyFile = "/crypt-ramfs/passphrase";
        keyFileTimeout = 1;
        allowDiscards = true;
      };
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

    def wait_for_disk_prompt(machine, settle: int = 8) -> None:
        # Before stage 2 there is no guest shell, so wait_until_tty_matches() and
        # Machine.sleep() cannot be used here. The unlock unit logs this line to the
        # kernel log (serial console) just before it starts the graphical prompt.
        import time
        machine.wait_for_console_text("wros-initrd-unlock: asking for the disk password")
        time.sleep(settle)  # host-side: let wros-boot take the terminal and draw

    def unlock_at_prompt(machine, keys: str, attempts: int = 3) -> None:
        # Type the disk password at the graphical prompt and check it was taken. Once in a
        # loaded run the keys never reached the prompt, and with nothing typing again the guest
        # rebooted to the same prompt until the test's hour ran out. The unlock unit logs the
        # hand-over to the kernel log (serial console).
        for attempt in range(attempts):
            machine.send_chars(keys)
            machine.send_key("ret")
            try:
                machine.wait_for_console_text("password handed to systemd-cryptsetup", timeout=60)
                return
            except Exception:
                print(f"disk password not taken (attempt {attempt + 1}); typing it again")
        raise AssertionError("the disk password was never taken at the graphical prompt")

    def wait_for_greeter(machine, settle: int = 5) -> None:
        # The login screen is drawn on DRM, so tty1 holds no text to match. Wait for the
        # state instead: greetd running its greeter session and wros-greeter alive.
        import time
        machine.wait_for_unit("greetd.service")
        machine.wait_until_succeeds("loginctl list-sessions --no-legend | grep -q _greetd")
        machine.wait_until_succeeds("pgrep -x wros-greeter")
        time.sleep(settle)

    import re
    import shlex
    import time

    # /dev/vcs holds font glyph codes; /dev/vcsu holds the characters, including the
    # highlight marker of the selected row.
    SCREEN_PY = (
        "import os;f=os.open('/dev/tty%s',os.O_RDONLY|os.O_NOCTTY);"
        "c=os.get_terminal_size(f).columns;"
        "d=open('/dev/vcsu%s','rb').read().decode('utf-32-le');"
        "print(chr(10).join(d[i:i+c] for i in range(0,len(d),c)))"
    )

    def screen(machine, tty="1"):
        return machine.succeed("python3 -c " + shlex.quote(SCREEN_PY % (tty, tty)))

    def highlighted(machine):
        rows = []
        for line in screen(machine).splitlines():
            at = line.find("▸ ")
            while at >= 0:
                rows.append(line[at + 2:].split("│", 1)[0].strip())
                at = line.find("▸ ", at + 2)
        return rows

    def choose(machine, pattern, limit=45):
        regex = re.compile(pattern, re.I)
        if any(regex.search(row) for row in highlighted(machine)):
            return
        machine.send_key("home")
        time.sleep(0.5)
        for _ in range(limit):
            if any(regex.search(row) for row in highlighted(machine)):
                return
            machine.send_key("down")
            time.sleep(0.4)
        raise Exception(f"could not select {pattern!r}; highlighted {highlighted(machine)!r}")

    def to_editor(machine) -> None:
        # rc13 (os/tests/rc_vm_phases.py grim_editor): the session may open on the dashboard,
        # where Ctrl-K is not Find anything. Esc closes the dashboard; in the editor Esc would
        # open the menu bar instead, so it is pressed only when the dashboard is showing.
        lines = screen(machine).splitlines()
        if lines and "Resume writing" in chr(10).join(lines) and "GRIMOIRE" not in lines[0]:
            machine.send_key("esc")
        machine.wait_until_tty_matches("1", "GRIMOIRE")

    def restart(machine) -> None:
        # Machine.reboot() sends Ctrl-Alt-Delete, which WrOS ignores on purpose
        # (os/config/60-wros-reboot-guard.conf). Power off through the shell and start again.
        machine.shutdown()
        machine.start(allow_reboot=True)

    def log_in(machine, username: str, password: str, *, colemak: bool, console: bool = False) -> None:
        # crates/wros-greeter/src/form.rs: the form opens on an empty username field; Enter
        # moves to the password and Enter there submits. Down in the username field cycles
        # saved accounts, so it is not used. For a console login the writer's own session
        # setting is used (docs/GRAPHICAL_SESSION.md: session "console" keeps text), which
        # puts the editor on tty1 as text that the tests can read. The greeter's switcher is
        # not used: under the native input path Shift-Tab did not reach it.
        keys = colemak_keys if colemak else (lambda text: text)
        if console:
            home = f"/home/{username}"
            machine.succeed(
                f"install -d -m 0700 -o {username} {home}/.config {home}/.config/wros && "
                f"printf '%s' '{{\"version\":1,\"session\":\"console\"}}' > {home}/.config/wros/session.json && "
                f"chown {username} {home}/.config/wros/session.json"
            )
        wait_for_greeter(machine)
        machine.send_chars(keys(username))
        machine.send_key("ret")
        machine.send_chars(keys(password))
        machine.send_key("ret")
        machine.wait_until_succeeds(f"pgrep -u {username} grimoire", timeout=120)

    def disk_identity(vm, name="vdb"):
        """What the installer session records about the disk the writer chose."""
        return {key: vm.succeed(f"cat /sys/class/block/{name}/{key}").strip()
                for key in ("dev", "size", "diskseq")}

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
            "target_identity": disk_identity(installer_vm),
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
        installer_vm.succeed("test -f /run/wros-installer/target/boot/EFI/BOOT/BOOTX64.EFI")
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
