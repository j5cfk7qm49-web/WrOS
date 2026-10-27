# First-install keyboard adoption — experimental

Issue #24 identified a deterministic integration defect: the default WrOS
QWERTY configuration ran after console-setup and overwrote the layout selected
in Debian Installer. The correction is a separate increment based on the
experimental installer PR #21; it is not present in the clean ISO built at
`59cc34f`.

`install-files.sh` creates `installer-keyboard-pending` only when staging a new
installer profile with no existing keyboard configuration. The boot helper
adopts exactly Debian's supported `us` QWERTY/Colemak/DH variants before tty1
login. It reads the fixed `/etc/default/keyboard` as data, without shell sourcing
or expansion. The existing version-1 keyboard schema, fixed layout IDs and
15-second independent rollback timer are retained.

Only the exact initial configuration can be adopted. Existing committed layouts
win, including a crash after the durable adoption rename. Unsupported, multiple,
missing or malformed Debian layout settings leave the current system map intact.
Preview is refused while that marker remains because the helper cannot safely
restore an unregistered system map. A dangling/symlinked marker is rejected.
This does not implement arbitrary custom-map registration.

All **19 focused keyboard safety tests** passed as a normal Debian user. The
full **81-method Python suite** passed with zero skips and no external network
interface in **285.484 seconds**, including real UI/PTY/model/runtime tests.
ShellCheck passed for the staging script. These do not qualify a new ISO.

`os/tests/keyboard_vm.py` injects the exact helper and a VM-only probe into a new
copy-on-write layer over the existing isolated demo artifact. It accepts no
host target, exposes no NIC/shared directory, uses no account credential and
checks the original artifact's whole-file SHA-256 before and after. It exercises
actual boot-service adoption, Linux `dumpkeys`/`loadkeys`, systemd timer rollback,
committed-layout preservation and refusal to replace an unregistered system map.
Its results are still pending. This fixture tests helper integration with a
simulated Debian-selected configuration, not Debian Installer's own keyboard
question, the corrected shipping ISO or physical keyboard behavior.

Run it as a normal Debian user after creating the isolated appliance artifact:

```sh
# Builder prerequisite: initramfs-tools-core (lsinitramfs), QEMU, OVMF and cpio.
python3 os/tests/keyboard_vm.py
```

The first fixture attempts failed before exercising the new helper: the added
initramfs stage needed Debian's explicit `ORDER` file and klibc's mount requires
both the virtual device and mount point. Those failed runs remain retained.
The fixture refuses to replace any existing local-bottom `ORDER` file.

Issue #24 and installer #3 remain open until their actual guest/installer
acceptance passes. QWERTY/Colemak-DH physical ANSI/ISO, modifier, recovery and
suspend/resume checks still require authorized hardware.
