{ config, lib, pkgs, wrosPkgs, wrosRoot, ... }:
let
  hp = wrosPkgs.paths.helperPaths;

  wrosPolkitPolicies = pkgs.runCommand "wros-polkit-policies" {} ''
    mkdir -p "$out/share/polkit-1/actions"

    cat > "$out/share/polkit-1/actions/org.wros.boot-settings.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS — Writer's OS</vendor>
  <action id="org.wros.boot-settings">
    <description>Choose the WrOS start screen and its colours</description>
    <message>Authorize a WrOS start-screen change</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.bootSettings}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.music-signin.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS</vendor>
  <action id="org.wros.music-signin">
    <description>Let a phone hand its Spotify login to WrOS for three minutes</description>
    <message>Authorize Spotify sign-in from a phone on this network</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.musicSignin}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.clock.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS</vendor>
  <action id="org.wros.clock">
    <description>Set the WrOS local clock with network time disabled</description>
    <message>Authentication is required to set the local clock</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.clock}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.display.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS — Writer's OS</vendor>
  <action id="org.wros.display">
    <description>Preview or confirm manual screen warmth</description>
    <message>Authorize a WrOS display change</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.display}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.installer.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS — Writer's OS</vendor>
  <action id="org.wros.installer">
    <description>Start the supervised WrOS encrypted live installer on console tty7</description>
    <message>Authorize starting the WrOS live installer</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.installSession}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.keyboard.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS project</vendor>
  <action id="org.wros.keyboard">
    <description>Preview or confirm a WrOS console keyboard layout</description>
    <message>Authorize a keyboard layout change</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>auth_admin</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.keyboard}</annotate>
  </action>
  <action id="org.writingdeck.keyboard">
    <description>Preview or confirm a WrOS console keyboard layout</description>
    <message>Authorize a keyboard layout change</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>auth_admin</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.keyboardCompat}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.lock.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="org.wros.lock">
    <description>Lock the local WrOS writing session</description>
    <message>Authorization is required to lock the local writing console</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.lock}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.import.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS</vendor>
  <action id="org.wros.import.media">
    <description>Read a USB stick to import a book</description>
    <message>Authorize reading a USB stick for book import</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.importMedia}</annotate>
  </action>
</policyconfig>
EOF

    cat > "$out/share/polkit-1/actions/org.wros.update.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>WrOS — Writer's OS</vendor>
  <action id="org.wros.update.stage">
    <description>Stage a reviewed WrOS update bundle for the next restart</description>
    <message>Authorize staging a WrOS update</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateStage}</annotate>
  </action>
  <action id="org.wros.update.cancel">
    <description>Cancel a staged WrOS update before it is applied</description>
    <message>Authorize cancelling a staged WrOS update</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateCancel}</annotate>
  </action>
  <action id="org.wros.update.repair">
    <description>Repair interrupted WrOS package state from retained authenticated archives</description>
    <message>Authorize repairing WrOS package state</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateRepair}</annotate>
  </action>
  <action id="org.wros.update.media">
    <description>Mount the WROS-UPDATE media read-only to look for update bundles</description>
    <message>Authorize reading WrOS update media</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateMedia}</annotate>
  </action>
  <action id="org.wros.update.fetch">
    <description>Download the newest signed WrOS update from the fixed WrOS update server</description>
    <message>Authorize downloading a WrOS update</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateFetch}</annotate>
  </action>
  <action id="org.wros.update.apply">
    <description>Install a staged app-only WrOS update now, restarting only the writing app</description>
    <message>Authorize installing a staged WrOS update now</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateApply}</annotate>
  </action>
  <action id="org.wros.update.rollback">
    <description>Go back to the previous WrOS version this computer installed</description>
    <message>Authorize going back to the previous WrOS version</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateRollback}</annotate>
  </action>
  <action id="org.wros.update.healthy">
    <description>Report that a new WrOS version started the writing app</description>
    <message>Authorize reporting that WrOS started</message>
    <defaults><allow_any>no</allow_any><allow_inactive>no</allow_inactive><allow_active>no</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">${hp.updateHealthy}</annotate>
  </action>
</policyconfig>
EOF
  '';
in
{
  security.polkit.enable = true;

  environment.systemPackages = [ wrosPolkitPolicies ];

  security.polkit.extraConfig = ''
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-boot-settings.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-clock.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-display.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-keyboard.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-lock.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-music.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-timezone.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/50-wros-update.rules"}
    ${builtins.readFile "${wrosRoot}/os/config/51-wros-greeter-keyboard.rules"}

    // Joining and forgetting Wi-Fi networks (#141). Debian builds NetworkManager with
    // modify_system, so the person at the laptop never needed a password for this; NixOS's build
    // asks for admin authentication, and nmcli's agent then fails (its helper path,
    // /run/wrappers/bin/polkit-agent-helper-1, is not installed now that polkit's helper is
    // socket-activated). Grant exactly what the app's nmcli calls need, to the active local
    // session of a wros member, as Debian did.
    polkit.addRule(function(action, subject) {
        var actions = ["org.freedesktop.NetworkManager.settings.modify.system",
                       "org.freedesktop.NetworkManager.network-control",
                       "org.freedesktop.NetworkManager.enable-disable-wifi",
                       "org.freedesktop.NetworkManager.wifi.scan"];
        if (actions.indexOf(action.id) >= 0 && subject.local && subject.active &&
            subject.isInGroup("wros")) {
            return polkit.Result.YES;
        }
    });
  '';
}
