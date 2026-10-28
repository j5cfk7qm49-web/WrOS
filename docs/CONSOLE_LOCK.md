# Manual console locking

This increment is under qualification. Do not use it as evidence for physical
security or sleep/resume. Suspend, automatic idle/lid sleep and all five sleep
targets remain disabled.

## Maintained authentication and console ownership

Settings → System → Battery and power offers **Lock writing session** only in
an installed, non-root tty1 writing session with the required local payload.
The marked passwordless VM demo does not offer it. Both editors must save
successfully, with no dirty scene, stale tree or unresolved source conflict.
The existing conflict/rescue behavior is retained.

The main thread retires and reaps the separate input reader, discards queued
events and partial parser state, and holds its single-use pause proof. It clears
the old console and stops rendering and terminal queries. It keeps tty1
non-echoing while starting the fixed polkit helper. The editor never prompts for
or receives an unlock password.

The root-owned helper requires the direct installed editor parent, matching
non-root UIDs, an active local logind user session on seat0/tty1, tty1 descriptors,
a password-bearing local writer and a locked root account. It checks fixed
root-owned program/module/PAM files and the reserved tty8 service masks.
It does not accept command arguments, paths or user configuration.

Debian `openvt` allocates unused tty8, switches to it and waits for Debian
`vlock-main all nosysrq`. The user-shell vlock wrapper is never executed, so
user `.vlockrc`, environment-selected plugins and shell expansion are excluded.
The helper uses a cleared environment and a fixed existing-session PAM stack
including Debian `common-auth`. Authentication is performed by maintained
vlock/libpam; WrOS does not implement password verification or read shadow data.

Readiness requires the actual vlock child, active tty8, `VT_PROCESS` ownership,
echo disabled and SysRq disabled. Merely launching a process or requesting the
`all` plugin is insufficient. The helper returns success only after readiness
was observed and the maintained child exited successfully after authentication.
The editor then flushes typeahead, restores its display and starts a fresh
acknowledged input reader. The same in-memory editor retains its writing context.
Failure leaves the proof unconsumed and closes the writing session through the
existing save/rescue path.

## Limits and verification

`all` uses Linux VT release/acquire ownership, not a global `VT_LOCKSWITCH` bit.
The kernel resets that mode when its owning process dies. A different protected
VT keeps locker credentials separate from tty1 if the editor exits. Qualification
must test actual VT switching, wrong/right PAM, reader absence, original console
stability, context restoration and process-death behavior; pure policy tests do
not establish those properties.

This does not add account enrollment, remote or graphical locking, idle locking,
lock-before-suspend, hibernation, disk encryption, a signed release or physical
hardware qualification. Root/admin changes and physical forced power-off remain
outside the session-lock boundary. The earlier delivered development ISO
predates this work and native timezone selection.

Maintained interfaces: Debian [vlock-main](https://manpages.debian.org/trixie/vlock/vlock-main.8.en.html),
[plugins](https://manpages.debian.org/trixie/vlock/vlock-plugins.5.en.html) and
[openvt](https://manpages.debian.org/trixie/kbd/openvt.1.en.html).
