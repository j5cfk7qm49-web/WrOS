{ pkgs, lib, src }:
# WrOS boot-menu branding, drawn from the owner's logo sketch (crates/wros-shell/src/logo.rs) so the
# boot menus, the start screen and the dashboard all show the same logo:
#   $out/grub-theme/   a GRUB theme: the logo above a centred menu, in the Grimoire theme's colours,
#                      with DejaVu Sans Mono (already the system font). Used by the installed
#                      system's GRUB and by the live medium's UEFI menu.
#   $out/bios-menu.png the live medium's legacy-BIOS menu background (640x480, logo at the top).
# Only the logo and the drawing script are inputs, so other edits to the tree don't rebuild it.
let
  inputs = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/crates/wros-shell/src/logo.rs")
      (src + "/nixos/scripts/wros-logo-png")
    ];
  };
  font = "DejaVu Sans Mono Regular 16";
  # The logo is 35 x 17 characters; at 8 x 16 pixels a character it is 280 x 272.
  themeTxt = pkgs.writeText "theme.txt" ''
    title-text: ""
    desktop-color: "#000000"
    terminal-font: "${font}"
    message-font: "${font}"
    message-color: "#dedcd4"
    message-bg-color: "#000000"

    + image {
      left = 50%-140
      top = 6%
      width = 280
      height = 272
      file = "logo.png"
    }

    + boot_menu {
      # Wide enough for the live medium's entry, "Start WrOS <version> (try it, or install it
      # from the dashboard)": 66 characters of about 10 pixels.
      left = 50%-360
      width = 720
      top = 6%+300
      height = 88%-300
      item_font = "${font}"
      selected_item_font = "${font}"
      item_color = "#dedcd4"
      selected_item_color = "#d6ad60"
      item_height = 26
      item_padding = 0
      item_spacing = 4
      icon_width = 0
      icon_height = 0
      item_icon_space = 0
      scrollbar = false
    }

    + label {
      id = "__timeout__"
      left = 0
      width = 100%
      top = 100%-48
      height = 20
      align = "center"
      font = "${font}"
      color = "#6c6c7a"
      text = "Starting in %d seconds"
    }

    + label {
      left = 0
      width = 100%
      top = 100%-24
      height = 20
      align = "center"
      font = "${font}"
      color = "#6c6c7a"
      text = "Up and down choose, Enter starts"
    }
  '';
in
pkgs.runCommand "wros-branding" {
  nativeBuildInputs = [ pkgs.python3 pkgs.grub2 ];
} ''
  mkdir -p "$out/grub-theme"
  python3 -I ${inputs}/nixos/scripts/wros-logo-png ${inputs}/crates/wros-shell/src/logo.rs \
    "$out/grub-theme/logo.png" 8 16
  python3 -I ${inputs}/nixos/scripts/wros-logo-png ${inputs}/crates/wros-shell/src/logo.rs \
    "$out/bios-menu.png" 8 14 640 480 24
  grub-mkfont -s 16 -o "$out/grub-theme/dejavu-sans-mono-16.pf2" \
    ${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSansMono.ttf
  cp ${themeTxt} "$out/grub-theme/theme.txt"
''
