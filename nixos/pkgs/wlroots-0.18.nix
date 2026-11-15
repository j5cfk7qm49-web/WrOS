{ pkgs }:
# The vendored cage (os/vendor/cage) is written against wlroots 0.18, which nixpkgs removed in
# favour of 0.19 and 0.20. Build 0.18.3, the last 0.18 release, with the 0.19 recipe.
# The hash was produced by `nix flake prefetch gitlab:wlroots/wlroots/0.18.3?host=gitlab.freedesktop.org`.
pkgs.wlroots_0_19.overrideAttrs (old: rec {
  version = "0.18.3";
  src = pkgs.fetchFromGitLab {
    domain = "gitlab.freedesktop.org";
    owner = "wlroots";
    repo = "wlroots";
    rev = version;
    hash = "sha256-D8RapSeH+5JpTtq+OU8PyVZubLhjcebbCBPuSO5Q7kU=";
  };
  # 0.18 predates libinput's LIBINPUT_SWITCH_KEYPAD_SLIDE; with -Werror the unhandled value stops
  # the build (backend/libinput/switch.c:32). Warnings stay visible, they just do not fail it.
  mesonFlags = (old.mesonFlags or [ ]) ++ [ "-Dwerror=false" ];
})
