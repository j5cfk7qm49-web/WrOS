{ config, lib, pkgs, ... }:
let
  drivers = {
    iwlwifi = { modules = "net/wireless/intel/iwlwifi/**/*.ko*"; fallback = "iwlwifi*"; };
    rtlwifi.modules = "net/wireless/realtek/rtlwifi/**/*.ko*";
    rtw88.modules = "net/wireless/realtek/rtw88/**/*.ko*";
    rtw89.modules = "net/wireless/realtek/rtw89/**/*.ko*";
    ath9k_htc.modules = "net/wireless/ath/ath9k/ath9k_htc.ko*";
    ath10k.modules = "net/wireless/ath/ath10k/**/*.ko*";
    ath11k.modules = "net/wireless/ath/ath11k/**/*.ko*";
    brcmfmac = { modules = "net/wireless/broadcom/brcm80211/brcmfmac/**/*.ko*"; fallback = "brcm"; };
    mt76 = { modules = "net/wireless/mediatek/mt76/**/*.ko*"; fallback = "mediatek"; };
    i915.modules = "gpu/drm/i915/**/*.ko*";
  };

  kernelModules = lib.getOutput "modules" config.boot.kernelPackages.kernel;
  firmwareDir = "${pkgs.linux-firmware}/lib/firmware";
  lists = pkgs.runCommand "wros-firmware-lists" { nativeBuildInputs = [ pkgs.kmod ]; } ''
    mkdir -p "$out"
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList
      (driver: { modules, fallback ? driver }: ''
        bash ${../../tools/firmware-lists.sh} ${kernelModules}/lib/modules ${firmwareDir} \
          ${lib.escapeShellArgs [ driver modules fallback ]} > "$out/${driver}.txt"
      '') drivers)}
    grep '^i915/glk_' "$out/i915.txt" > "$out/i915-glk.txt"
  '';

  # Copy each individual list into the store so changes to other lists or the
  # rest of the source tree do not change this package's store path.
  package = driver:
    let
      list = ../firmware + "/${driver}.txt";
      raw = pkgs.runCommand "wros-firmware-${driver}" { } ''
        test -s ${list}
        while IFS= read -r file || [[ -n $file ]]; do
          install -Dm644 -- "${firmwareDir}/$file" "$out/lib/firmware/$file"
        done < ${list}
      '';
      compression = config.hardware.firmwareCompression;
      compressed = if compression == "zstd" then pkgs.compressFirmwareZstd raw
        else if compression == "xz" then pkgs.compressFirmwareXz raw
        else raw;
    in compressed.overrideAttrs (old: {
      passthru = (old.passthru or { }) // { compressFirmware = false; };
    });
in
{
  options.wros.firmware = lib.mkOption {
    type = lib.types.attrsOf lib.types.package;
    readOnly = true;
    description = "WrOS firmware packages built from the committed per-driver file lists.";
  };

  config = {
    wros.firmware = lib.genAttrs (lib.attrNames drivers ++ [ "i915-glk" ]) package;

    system.build.wrosFirmwareLists = lists;
    system.build.wrosFirmwareListsCurrent = pkgs.runCommand "wros-firmware-lists-current" { } ''
      diff -ru ${../firmware} ${lists}
      touch "$out"
    '';
  };
}
