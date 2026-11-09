# RC10 menu, dialog and lock driver checks

The two new driven tests first failed because the menu action and lock view were
missing. The lock fixture had already reached its independent prompt. The same
run exposed a timing problem in the narrow-console test. The fixture now waits
for its first published view as well as the terminal screen.

All seven driven tests passed in Debian, including named menu and dialog
actions, the failed Wi-Fi scan, the headless OpenGL window, and two lock cycles
using fresh connections. Strict Clippy passed. The separate Python PTY test
passed its checks for the retired reader, a new reader after unlock, and
credential and physical typeahead isolation. Logs are in
`/home/builder/rc10-driver-*.log`.

The first native VM run (`graphical-drm-20261109T004136Z`) reached the real
password prompt. Its test client then tried to send a key on the connection
that unlock retired. The harness now retries read-only snapshots until the
writing view returns. The passing run (`graphical-drm-20261109T004446Z`, 112 s)
saw only the public status during authentication, rejected a wrong password,
returned to the same writer through three locks and suspend, and passed its
credential scans. The tested debug writer SHA-256 is
`6ebe1af80a5da00460de415a80ff02f51111fd7c0f9c67f2657b2cc2c7d6a89a`.
