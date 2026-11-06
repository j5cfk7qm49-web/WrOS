# Lemurs evaluation

The evaluation used Lemurs 0.4.0 on an encrypted Debian 13 VM updated to WrOS
rc6. The upstream x86-64 binary matched its published SHA-256. The VM ran on
a disposable overlay with networking disabled. Lemurs used tty1. Its TTY
session ran a login shell so the existing WrOS profile could start the writing app.
The experiment used a temporary systemd service in the VM. [Lemurs v0.4.0](https://github.com/coastalwhite/lemurs/releases/tag/v0.4.0)

Lemurs reads the console keymap in effect when it starts. The VM's Colemak
map was already loaded by the WrOS keyboard service. WrOS's own greeter also
provides a layout selector before authentication, a notice when it preserves
a layout, the shared Settings theme and the login milestone for recovery.

With the upstream PAM service (`lemurs-20261106T205622Z`) and the installed
greetd PAM service (`lemurs-20261106T205828Z`), the wrong-layout attempt was
rejected and the password stayed hidden. The correct Colemak password reached
PAM session setup, where `pam_loginuid` was refused permission to write the
login UID. The app did not start with either stack. [Related upstream login-UID report](https://github.com/coastalwhite/lemurs/issues/254)

On `lemurs-20261106T205955Z`, a VM-only PAM service used Debian's common
authentication, account, session and password stacks. Colemak authentication
opened the WrOS session with both Grimoire and its input reader running as the
writer on tty1. Ctrl+Q returned to Lemurs. The fixture password was absent
from the system logs. This establishes that Lemurs can launch the console
session with that PAM configuration. The production login UID policy needs
its own qualification before a greeter switch.

Keep the WrOS greeter for the console release. Revisit Lemurs as part of the
cage and foot evaluation in #92. That experiment should compare starting the
same cage session from greetd and Lemurs, with the same XKB layout, login and
boot-success milestones, console fallback, and lock on return from sleep.
The TTY test does not exercise the compositor or the graphical input path.
[Graphical session issue](https://github.com/j5cfk7qm49-web/WrOS/issues/92)
