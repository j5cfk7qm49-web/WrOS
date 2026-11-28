{ config, lib, pkgs, ... }:
let
  # One package per kernel driver. `modules` finds the driver's modules under kernel/drivers;
  # `fallback` is used only where the modules name no firmware; `also` adds chip folders the
  # driver loads but modinfo doesn't name.
  drivers = {
    iwlwifi = { modules = "net/wireless/intel/iwlwifi/**/*.ko*"; fallback = "iwlwifi*"; };
    iwlegacy = { modules = "net/wireless/intel/iwlegacy/**/*.ko*"; fallback = "iwlwifi-[34]9*"; };
    rtlwifi.modules = "net/wireless/realtek/rtlwifi/**/*.ko*";
    rtl8xxxu = { modules = "net/wireless/realtek/rtl8xxxu/**/*.ko*"; fallback = "rtlwifi/rtl8*u*"; };
    rtl8723bs = { modules = "staging/rtl8723bs/**/*.ko*"; fallback = "rtlwifi/rtl8723bs*"; };
    rtw88.modules = "net/wireless/realtek/rtw88/**/*.ko*";
    rtw89.modules = "net/wireless/realtek/rtw89/**/*.ko*";
    ath9k_htc.modules = "net/wireless/ath/ath9k/ath9k_htc.ko*";
    ath10k.modules = "net/wireless/ath/ath10k/**/*.ko*";
    # The laptop (PCIe) chips. modinfo doesn't name every one the driver loads, such as QCA2066.
    ath11k = { modules = "net/wireless/ath/ath11k/**/*.ko*"; also = "ath11k/QCA* ath11k/WCN6855"; };
    brcmfmac = { modules = "net/wireless/broadcom/brcm80211/brcmfmac/**/*.ko*"; fallback = "brcm"; };
    brcmsmac = { modules = "net/wireless/broadcom/brcm80211/brcmsmac/**/*.ko*"; fallback = "brcm/bcm43xx*"; };
    mt76 = { modules = "net/wireless/mediatek/mt76/**/*.ko*"; fallback = "mediatek"; };
    i915.modules = "gpu/drm/i915/**/*.ko*";
    amdgpu.modules = "gpu/drm/amd/**/*.ko*";
    radeon.modules = "gpu/drm/radeon/**/*.ko*";
    xe.modules = "gpu/drm/xe/**/*.ko*";
    # Intel also constructs chip-specific firmware names at run time.
    bt-intel = rec { modules = "bluetooth/bt@(intel|usb).ko*"; fallback = "intel/ibt-*"; also = fallback; };
    bt-realtek = { modules = "bluetooth/btrtl.ko*"; fallback = "rtl_bt"; };
    bt-qca = { modules = "bluetooth/btqca.ko*"; fallback = "qca"; };
    bt-broadcom = { modules = "bluetooth/btbcm.ko*"; fallback = "brcm/BCM*"; };
  };

  # The HP Stream 11's must-haves: its Wi-Fi (RTL8822BE) and Gemini Lake display firmware.
  stream = {
    rtw88 = "rtw88/rtw8822b_fw.bin";
    i915-glk = "i915/glk_dmc_ver1_04.bin";
  };

  kernelModules = lib.getOutput "modules" config.boot.kernelPackages.kernel;
  firmwareDir = "${pkgs.linux-firmware}/lib/firmware";
  lists = pkgs.runCommand "wros-firmware-lists" {
    nativeBuildInputs = [ pkgs.kmod pkgs.jq ];
    outputs = [ "out" "moduleNames" ];
  } ''
    set -o pipefail
    shopt -s globstar nullglob extglob
    mkdir -p "$out"

    writeModules() {
      local driver=$1 pattern=$2
      (
        cd ${kernelModules}/lib/modules
        IFS=
        modules=( */kernel/$pattern )
        if (( ''${#modules[@]} )); then
          modinfo -F name "''${modules[@]}"
        fi
      ) | jq -Rn --arg driver "$driver" \
        '{($driver): ([inputs | select(length > 0)] | unique)}' > "$driver.json"
    }

    ${lib.concatStringsSep "\n" (lib.mapAttrsToList
      (driver: { modules, fallback ? driver, also ? "" }: ''
        bash -O extglob ${../../tools/firmware-lists.sh} ${kernelModules}/lib/modules ${firmwareDir} \
          ${lib.escapeShellArgs [ driver modules fallback also ]} > "$out/${driver}.txt"
        writeModules ${lib.escapeShellArgs [ driver "drivers/${modules}" ]}
      '') drivers)}
    grep '^i915/glk_' "$out/i915.txt" > "$out/i915-glk.txt"
    writeModules sof 'sound/soc/sof/**/*.ko*'
    jq -s 'add | .["i915-glk"] = .i915' ./*.json > "$moduleNames"
  '';

  compress = raw:
    let
      compression = config.hardware.firmwareCompression;
      compressed = if compression == "zstd" then pkgs.compressFirmwareZstd raw
        else if compression == "xz" then pkgs.compressFirmwareXz raw
        else raw;
    in compressed.overrideAttrs (old: {
      passthru = (old.passthru or { }) // { compressFirmware = false; };
    });

  # Copy each individual list into the store so changes to other lists or the
  # rest of the source tree do not change this package's store path.
  package = driver:
    let
      list = ../firmware + "/${driver}.txt";
    in compress (pkgs.runCommand "wros-firmware-${driver}" { } ''
        test -s ${list}
        while IFS= read -r file || [[ -n $file ]]; do
          install -Dm644 -- "${firmwareDir}/$file" "$out/lib/firmware/$file"
        done < ${list}
      '');

  packages = lib.genAttrs (lib.attrNames drivers ++ [ "i915-glk" ]) package // {
    sof = compress pkgs.sof-firmware;
  };

  hashes = pkgs.runCommand "wros-firmware-hashes.json" {
    nativeBuildInputs = [ pkgs.nix pkgs.jq ];
    allowedReferences = [ ];
  } ''
    set -o pipefail
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (driver: pkg: ''
      hash=$(nix-store --dump ${pkg} | tee firmware.nar | sha256sum | cut -d' ' -f1)
      jq -n --arg driver ${lib.escapeShellArg driver} --arg hash "$hash" \
        --argjson size "$(stat -c %s firmware.nar)" \
        '{($driver): {nar_sha256: $hash, nar_size: $size}}' > ${driver}.json
    '') packages)}
    jq -s add ./*.json > "$out"
  '';

  # Discard the package paths' string context before passing them to the table builder.
  # Only the hash builder sees the packages as inputs, and its output contains just data.
  storePaths = builtins.unsafeDiscardStringContext (builtins.toJSON (lib.mapAttrs (_: toString) packages));
  initrdModules = builtins.toJSON (map (lib.replaceStrings [ "-" ] [ "_" ]) (
    config.boot.initrd.kernelModules ++ config.boot.initrd.availableKernelModules
  ));
  table = pkgs.runCommand "wros-firmware-table.json" {
    nativeBuildInputs = [ pkgs.jq ];
    allowedReferences = [ ];
  } ''
    jq -n --argjson paths ${lib.escapeShellArg storePaths} \
      --argjson initrdModules ${lib.escapeShellArg initrdModules} \
      --slurpfile hashes ${hashes} --slurpfile modules ${lists.moduleNames} '
      {format: 1, packages: ($paths | with_entries(
        .key as $driver | .value = ($hashes[0][$driver] + {
          store_path: .value,
          modules: $modules[0][$driver],
          initrd: any($modules[0][$driver][]; . as $module | $initrdModules | index($module) != null)
        })
      ))}' > "$out"
  '';
in
{
  options.wros.firmware = lib.mkOption {
    type = lib.types.attrsOf lib.types.package;
    readOnly = true;
    description = "WrOS firmware packages for kernel drivers, including Sound Open Firmware.";
  };

  config = {
    wros.firmware = packages;

    # A list regenerated after a kernel bump can't drop the Stream's files without stopping
    # every build of this system here.
    assertions = lib.mapAttrsToList (driver: file: {
      assertion = lib.elem file (lib.splitString "\n" (builtins.readFile (../firmware + "/${driver}.txt")));
      message = "nixos/firmware/${driver}.txt must list ${file}: the HP Stream 11 needs it.";
    }) stream;

    system.build.wrosFirmwareLists = lists;
    system.build.wrosFirmwareTable = table;
    system.build.wrosFirmwareListsCurrent = pkgs.runCommand "wros-firmware-lists-current" { } ''
      diff -ru ${../firmware} ${lists}
      touch "$out"
    '';
  };
}
