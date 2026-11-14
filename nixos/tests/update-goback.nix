{ pkgs, lib, version, wrosPackages, installedModule, liveModule }:
let
  encryptedDisk = import ./encrypted-disk.nix {
    inherit pkgs lib version wrosPackages installedModule liveModule;
  };

  # Build a real second system closure (0.1.0~rc15) and an older closure (0.1.0~rc12)
  # so the VM test exercises real store paths, real binary caches, and real generation switches.
  testKeysDir = ./keys;
  secretKeyFile = ./keys/test-signer.sec.asc;

  mkSignedBundle = { bundleName, bundleVersion, systemToplevel }:
    let
      unsigned = import ../pkgs/update-bundle.nix {
        inherit pkgs lib;
        version = bundleVersion;
        src = ../..;
        inherit systemToplevel;
      };
    in
    pkgs.runCommand "wros-signed-bundle-${bundleName}" {
      nativeBuildInputs = [ pkgs.gnupg ];
    } ''
      mkdir -p "$out"
      cp -r "${unsigned}/." "$out/"
      chmod -R u+w "$out"
      export GNUPGHOME="$TMPDIR/gnupg"
      mkdir -p -m 0700 "$GNUPGHOME"
      gpg --batch --import "${secretKeyFile}"
      fpr=$(gpg --batch --with-colons --fingerprint --list-secret-keys | awk -F: '/^fpr:/ { print $10; exit }')
      gpg --batch --yes --local-user "$fpr" --clearsign --output "$out/InRelease" "$out/Release"
    '';
in
pkgs.testers.runNixOSTest {
  name = "wros-update-goback";

  nodes = {
    installer = { nodes, ... }: { imports = [ (encryptedDisk.mkInstallerNode nodes.installed.system.build.toplevel) ]; };

    # Point only the test machine's update trust directory at nixos/tests/keys (U6, T5).
    installed = encryptedDisk.mkInstalledNode ({ ... }: {
      wros.updates.trustDir = testKeysDir;
    });

    updated = encryptedDisk.mkInstalledNode ({ lib, ... }: {
      wros.updates.trustDir = testKeysDir;
      environment.etc."wros/version".text = lib.mkForce "0.1.0~rc15\n";
      environment.etc."wros/update-test-generation".text = "rc15-next\n";
    });

    older = encryptedDisk.mkInstalledNode ({ lib, ... }: {
      wros.updates.trustDir = testKeysDir;
      environment.etc."wros/version".text = lib.mkForce "0.1.0~rc12\n";
    });
  };

  testScript = { nodes, ... }:
    let
      initialToplevel = nodes.installed.system.build.toplevel;
      updatedToplevel = nodes.updated.system.build.toplevel;
      olderToplevel = nodes.older.system.build.toplevel;

      bundleNext = mkSignedBundle {
        bundleName = "rc15";
        bundleVersion = "0.1.0~rc15";
        systemToplevel = updatedToplevel;
      };
      bundleSame = mkSignedBundle {
        bundleName = "same";
        bundleVersion = version;
        systemToplevel = initialToplevel;
      };
      bundleDowngrade = mkSignedBundle {
        bundleName = "downgrade";
        bundleVersion = "0.1.0~rc12";
        systemToplevel = olderToplevel;
      };
    in
    ''
      ${encryptedDisk.pythonPreamble}

      # 1. Build and boot the encrypted installed disk (trusting nixos/tests/keys)
      disk_img = build_encrypted_disk(
          installer,
          keyboard_layout="qwerty",
          luks_passphrase="update-disk-passphrase",
          username="writer",
          user_password="writer-update-password",
      )
      prepare_installed_disk(installed, disk_img)

      installed.start(allow_reboot=True)
      wait_for_disk_prompt(installed)
      installed.send_chars("update-disk-passphrase")
      installed.send_key("ret")
      installed.wait_for_unit("multi-user.target")
      installed.wait_for_unit("greetd.service")

      # Verify initial installed version and system profile link
      assert installed.succeed("cat /etc/wros/version").strip() == "${version}"
      orig_system = installed.succeed("readlink -f /run/current-system").strip()
      orig_profile = installed.succeed("readlink -f /nix/var/nix/profiles/system").strip()
      assert orig_system == orig_profile

      # Prepare bundles under /media/wros-update (an allowed removable-media prefix).
      # Note: The test driver's root shell is not the writer's active tty1 session, so
      # launchers are invoked as root with PKEXEC_UID=1000 set to the writer's UID (T5).
      installed.succeed("mkdir -p /media/wros-update")
      installed.succeed("cp -r ${bundleDowngrade} /media/wros-update/downgrade")
      installed.succeed("cp -r ${bundleSame} /media/wros-update/same")
      installed.succeed("cp -r ${bundleNext} /media/wros-update/rc15")

      # 2. Downgrade and equal-version bundles are refused (U3, T5). The engine refuses them at
      #    review (nixos/scripts/wros-update-engine.py verify_open_nixos_bundle), and staging
      #    re-verifies, so both steps must fail.
      UPDATE = "/run/current-system/sw/libexec/wros-update"
      STAGE = "PKEXEC_UID=1000 /run/current-system/sw/libexec/wros-update-stage"
      ANY_ID = "0" * 64

      out = installed.fail(f"{UPDATE} review /media/wros-update/downgrade 2>&1")
      assert "downgrade" in out, out
      installed.fail(f"{STAGE} /media/wros-update/downgrade {ANY_ID}")

      out = installed.fail(f"{UPDATE} review /media/wros-update/same 2>&1")
      assert "already installed" in out, out
      installed.fail(f"{STAGE} /media/wros-update/same {ANY_ID}")

      # 3. A bundle reviewed while intact and corrupted afterwards is refused at staging (T5)
      installed.succeed("cp -r /media/wros-update/rc15 /media/wros-update/tampered && chmod -R u+w /media/wros-update/tampered")
      tamper_review = json.loads(installed.succeed(f"{UPDATE} review /media/wros-update/tampered"))
      installed.succeed(
          "f=$(find /media/wros-update/tampered/cache/nar -name '*.nar.zst' | head -n 1) && printf 'corrupt' >> \"$f\""
      )
      installed.fail(f"{STAGE} /media/wros-update/tampered {tamper_review['review_id']}")

      # 4. Stage the valid signed 0.1.0~rc15 bundle via wros-update-stage and reboot to apply
      valid_review = json.loads(installed.succeed(f"{UPDATE} review /media/wros-update/rc15"))
      assert valid_review["packages"][0]["action"] == "upgrade", valid_review
      stage_res = json.loads(
          installed.succeed(
              f"{STAGE} /media/wros-update/rc15 {valid_review['review_id']}"
          )
      )
      assert stage_res["status"] == "staged", stage_res
      installed.succeed("test -L /system-update")
      installed.succeed("sync")

      # Reboot into system-update.target; wros-update-offline.service imports the binary cache,
      # switches /nix/var/nix/profiles/system, and triggers a reboot into the new generation.
      restart(installed)
      wait_for_disk_prompt(installed)
      installed.send_chars("update-disk-passphrase")
      installed.send_key("ret")

      # Wait for the offline update reboot into the new generation
      wait_for_disk_prompt(installed)
      installed.send_chars("update-disk-passphrase")
      installed.send_key("ret")
      installed.wait_for_unit("multi-user.target")
      installed.wait_for_unit("greetd.service")

      # Verify the system profile and running system switched to the new generation (0.1.0~rc15)
      assert installed.succeed("cat /etc/wros/version").strip() == "0.1.0~rc15"
      installed.succeed("test -f /etc/wros/update-test-generation")
      new_profile = installed.succeed("readlink -f /nix/var/nix/profiles/system").strip()
      new_current = installed.succeed("readlink -f /run/current-system").strip()
      assert new_profile == "${updatedToplevel}", (new_profile, "${updatedToplevel}")
      assert new_current == "${updatedToplevel}", (new_current, "${updatedToplevel}")
      installed.succeed("test ! -d /var/lib/wros-update/installed/cache")

      # 5. Stage rollback via the real wros-update-rollback launcher and reboot back to the previous generation
      rollback_res = json.loads(
          installed.succeed("PKEXEC_UID=1000 /run/current-system/sw/libexec/wros-update-rollback")
      )
      assert rollback_res["status"] == "staged" and rollback_res["rollback"] is True, rollback_res
      installed.succeed("test -L /system-update")
      installed.succeed("sync")

      restart(installed)
      wait_for_disk_prompt(installed)
      installed.send_chars("update-disk-passphrase")
      installed.send_key("ret")

      wait_for_disk_prompt(installed)
      installed.send_chars("update-disk-passphrase")
      installed.send_key("ret")
      installed.wait_for_unit("multi-user.target")
      installed.wait_for_unit("greetd.service")

      assert installed.succeed("cat /etc/wros/version").strip() == "${version}"
      restored_current = installed.succeed("readlink -f /run/current-system").strip()
      assert restored_current == orig_system, (restored_current, orig_system)
    '';
}
