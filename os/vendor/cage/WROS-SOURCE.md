# WrOS Cage integration

This is the complete top-level build source of Cage 0.2.0 (MIT), obtained from
Debian trixie's authenticated `cage 0.2.0-2` source package. No Debian patches
were present. The upstream archive is
`https://deb.debian.org/debian/pool/main/c/cage/cage_0.2.0.orig.tar.gz`, SHA-256
`9d3f659e0f19636a958f9e1bf4d22268d7e2705d7e2989888401ec443c1cb6c3`.
The Debian packaging archive SHA-256 is
`282bfeaa9c0d552749d4421ccdbe2fa7bc696bd81851aad1ff798dd8ab5de41b`.
The original `LICENSE` and copyright headers are retained.

WrOS changes in `cage.c`, `server.h`, `output.h` and `output.c` connect the
wlroots 0.18 gamma manager's set/destroy events to output frame commits. Failed
commits notify and release the control; destroying a control resets its LUT.
The private binary is installed as `/usr/lib/wros/cage`, leaving Debian's
`/usr/bin/cage` available. `meson.build` uses the stable `0.2.0+wros1` identifier
instead of the surrounding WrOS Git branch. The hardened build disables Meson
dependency downloads and maps source paths to a stable prefix.

References: [Cage 0.2.0](https://github.com/cage-kiosk/cage/tree/v0.2.0),
[wlroots gamma API](https://sources.debian.org/src/wlroots/0.18.2-3/include/wlr/types/wlr_gamma_control_v1.h/).
