# Installer configuration

WrOS installs from the NixOS live image. This page describes the installer
and records how the earlier Debian releases installed.

## The NixOS installer

Three scripts in [nixos/scripts](../../../nixos/scripts) do the work. NixOS
packages them with Nix store paths in
[helpers.nix](../../../nixos/pkgs/helpers.nix):

- [wros-install-session](../../../nixos/scripts/wros-install-session) is started
  from the live writing session. It checks that its caller is the live Grimoire
  session on tty1, records the chosen disk's identity, and runs the installer
  frontend on tty7 as the locked `_wros-installer` account.
- The frontend is the `wros-installer` Rust program in
  [crates/wros-installer](../../../crates/wros-installer). It asks for the
  keyboard layout, the computer's name, the writer's name, user name and
  password, the disk passphrase, and confirmation before the disk is erased.
- [wros-installer-backend](../../../nixos/scripts/wros-installer-backend) speaks
  the same line-based cdebconf passthrough protocol the frontend used on Debian,
  so the frontend is unchanged. It passes the answers to the install core over
  stdin, never in command arguments or the environment.
- [wros-install-core](../../../nixos/scripts/wros-install-core) runs as root. It
  checks again that the disk is the one chosen and that nothing on it is in use.
  Then it creates a GPT disk with a 512 MiB EFI partition and a LUKS2 container
  holding ext4, and installs the prebuilt system closure from the live medium
  without a network. It locks root, creates the writer account and installs the
  boot loader last.

The keyboard layout can be QWERTY, Colemak, Colemak-DH ANSI or Colemak-DH ISO.
The install core saves it to `/boot/wros-boot/keyboard.json`, and the systemd
initrd loads it before the disk passphrase prompt. The prompt therefore uses
the layout chosen during installation.

The [install-encrypted VM check](../../../nixos/tests/install-encrypted.nix)
covers installation, refusal of the wrong or a mounted disk, and retrying after
an interrupted install, all on disposable disks. Installing on a physical
laptop is checked separately for each release.

The retained `native-crypto-passphrase` hook is a fixture used by the
[historical Debian VM harness](../../tests/native_installer_vm.py).

## History

Up to rc13, WrOS used Debian Installer as its disk and account engine. The
native frontend reached it through Debian's cdebconf passthrough module
(source version 0.280, original BSD notice retained). The module sent
multi-line text that broke the line-based protocol, so WrOS patched it to
escape backslashes and newlines; the frontend decodes those fields once.
Debian's keymap question had no Colemak, so a local udeb added `us(colemak)`,
`us(colemak_dh)` and `us(colemak_dh_iso)`, checked byte for byte against the
installer's own `ckbcomp-mini`. Another hook set `KEYMAP=y` so the disk
passphrase prompt used the chosen layout.
[INSTALLER_BACKEND](../../../docs/archive/INSTALLER_BACKEND.md) and
[LIVE_INSTALLER_IMAGE](../../../docs/archive/LIVE_INSTALLER_IMAGE.md) keep the
details and their test results. Those results do not qualify the NixOS installer.
