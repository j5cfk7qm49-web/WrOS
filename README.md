# WrOS — Writer’s OS

WrOS is a NixOS 26.05 system for writing offline. It starts in Grimoire, a
full-screen writing desk with a scene outline, a second editor for notes,
history, and tools for checking and exporting a manuscript. Books are folders
of Markdown files. Wi-Fi, updates, and optional backups and sync live in
Settings. [Writing features](docs/FEATURE_MATRIX.md) · [Backups and sync](docs/SYNC_AND_BACKUP.md)

## Writing and getting help

Press `Esc` for the menu, `F12` for the WrOS dashboard, `F1` for help, and `F9`
for themes. The dashboard opens the interactive tutorial. Press `d` in the
tutorial to create a separate practice book. The 50 lessons cover writing and
system tasks, including the lid choice, battery warnings, and merge practice.
[Keyboard shortcuts](docs/KEYBOARD_SHORTCUTS.md) · [Tutorial](docs/TUTORIAL.md)

## Startup and recovery

WrOS defaults to graphical disk unlock, loading, login, and writing. Press
`Esc` at the graphical disk password prompt for systemd-cryptsetup's plain
prompt. The boot menu offers **WrOS (text start)** and **WrOS recovery**. On an installed
system, `Ctrl+Alt+F2` opens an authenticated maintenance console. The writing
session hands lock authentication to vlock and PAM.
[Graphical screens](docs/GRAPHICAL_SESSION.md) · [Recovery guide](docs/RC_RESCUE.md)

## Installing and updating

The installation guide covers the UEFI live session, encrypted install, and
disposable VM profile. On an existing installation, open **Settings › Updates
and recovery › Updates** to review a signed update and stage it for restart.
rc14 and rc15 cannot install updates from Settings; follow
[Update an installed system](docs/INSTALLATION.md#update-an-installed-system).
Debian rc13 and earlier require backup, a fresh NixOS install, and restore.
The release checklist records the build, VM qualification and HP Stream checks.
[Installation](docs/INSTALLATION.md) · [Releases](https://github.com/j5cfk7qm49-web/WrOS/releases) · [Release checklist](docs/RELEASE_CHECKLIST.md)

## Building and testing

Use an x86_64 Linux builder with Nix and flakes enabled for the OS. Build the
image with `nix build .#wros-iso`. For a local terminal build of the editor,
run the following with a disposable book or demo directory.

```sh
cargo run --locked --no-default-features -- wros tutorial-demo /path/to/new-demo
cargo run --locked --no-default-features -- /path/to/new-demo
```

The flake defines the NixOS software and VM checks. Run
`nix build .#checks.x86_64-linux.python-suite .#checks.x86_64-linux.rust-suite`
and follow the NixOS qualification guide for the VM checks. `tools/rc-check.sh`
and the RC10 VM chain remain Debian-era checks.
[Testing](docs/TESTING.md) · [NixOS qualification](docs/NIXOS_QUALIFICATION.md) · [Documentation index](docs/README.md)

## Grimoire and licensing

WrOS builds on GrimoireTUI by Josh King and Catfinity Studios. Copyright (c)
2026 Josh King. The MIT notice and the third-party software and content
notices are retained. [GrimoireTUI](https://grimoiretui.com) · [MIT license](LICENSE) · [Notices](docs/CONTENT_LICENSES.md)
