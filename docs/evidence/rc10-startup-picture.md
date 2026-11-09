# RC10 graphical startup picture

The writing session shows the owner's four-box WrOS design as a 35 by 17 cell
picture. The embedded PNG is 70 by 68 pixels. The static picture has a 180 ms
timer. A key or paste is replayed after the first usable writing frame.
A click dismisses the picture. The writer skips the picture on the console,
in low-power mode, or if the display is too small. The login screen keeps
its existing character logo. [Native VM capture](rc10-startup-picture.png)

The splash unit tests and the native raster test failed before implementation
and passed after it. The old font rendered different Braille patterns with
the same missing-glyph box. The new font preserves the old character mapping,
outlines and spacing and adds all 256 Braille patterns. In the three-style
native font atlas, the old and new text cells match except for 74 check-mark
pixels whose channels differ by at most 4 out of 255. Strict Clippy and the
four driven tests passed. Logs are in `/home/builder/rc10-splash-*.log`.

Each disposable installed VM captured the picture before the writer opened.
The runs checked the graphical login form, repeated visible lock and password
screens, suspend and wake, return to the same writer and the credential scans.

| Renderer | Seconds | Passing run |
| --- | ---: | --- |
| drm | 122.8 | `/home/builder/rc-vm-runs/graphical-drm-20261109T002035Z` |
| foot | 111.5 | `/home/builder/rc-vm-runs/graphical-foot-20261109T002238Z` |
| wgpu | 109.8 | `/home/builder/rc-vm-runs/graphical-wgpu-20261109T002429Z` |

The tested writer SHA-256 is `2ae970a6ade1f005f35442771461b6769e33ff2223418e9bef0bfe02f6d8a338`.
The tested greeter SHA-256 is `5ac2275ab37c90928801eada2f2e63e53672e1415cd99ecc29b7627f791af65b`.
The evidence collector verified the relevant source and asset bytes against
the Debian build tree. Physical display and final release checks are pending.
