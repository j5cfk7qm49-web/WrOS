# Linux foundation decision

WrOS 0.1.0~rc16 uses NixOS 26.05 on `x86_64-linux`.
[flake.nix](../flake.nix) defines the installed system, live image, packages and
checks. [base.nix](../nixos/modules/base.nix) declares the base and release
identity. The flake pins Nixpkgs and the Rust overlay; `flake.lock` records the
resolved inputs.

## Comparison

The current decision follows the system model implemented in the repository.
These are design reasons, not measured boot, memory or battery comparisons.

| Requirement | NixOS implementation | Boundary |
|---|---|---|
| Declarative services | [Installed modules](../nixos/modules/installed.nix) compose the base, hardware, networking, audio, session, policy, boot, recovery and updates | Service and package changes belong in the modules |
| System generations | The [update engine](../nixos/scripts/wros-update-engine.py) switches `/nix/var/nix/profiles/system` and installs boot entries | `/run/current-system` identifies the running closure |
| Go back and repair | The engine selects the previous generation and can verify the store and restore the current boot configuration | Repair refuses a failed store-integrity check; it does not invent missing store contents |
| Offline installation | The [live module](../nixos/modules/live.nix) carries the complete installed-system closure | The [installer core](../nixos/scripts/wros-install-core) copies that closure to the selected encrypted disk |
| Offline updates | The [bundle package](../nixos/pkgs/update-bundle.nix) exports a local binary cache and closure manifest | The engine authenticates signed metadata and checks cache hashes before import |

Generations and rollback keep the previous system available without an in-place
package transaction. Declarative modules make the installed system and live
image use the same package definitions. Carrying the closure on the medium makes
installation independent of a network package resolver.

## Concrete package evaluation

[base.nix](../nixos/modules/base.nix) includes Grimoire, the native greeter and
startup program, patched Cage, Foot, keymaps, the prose-tools runtime and helpers.
It also includes tools used by the writing workflows, such as Git, restic,
rclone, WordNet and eSpeak NG. The Python runtime is built by
[prose-runtime.nix](../nixos/pkgs/prose-runtime.nix) from the shared lock.

[paths.nix](../nixos/pkgs/paths.nix) supplies Nix store tool, XKB and Compose
paths to the application and helper policies.
[keymaps.nix](../nixos/pkgs/keymaps.nix) compiles the four supported layouts from
Nixpkgs XKB data. Normal on-device package administration is disabled in the
base module; system changes use the packaged update path.

## Image and reproducibility strategy

The installed system is `nixosConfigurations.wros`; the live image is
`.#wros-iso`. Both are defined in [flake.nix](../flake.nix). The live image
contains the installed closure, and `.#wros-update-bundle` exports the system's
binary cache. A pinned build definition does not by itself establish identical
image bytes across builds.

[flake.nix](../flake.nix) exposes installation, boot/login, lock, keyboard,
update/go-back and recovery VM checks, plus Rust and Python suites. The
[Nix workflow](../.github/workflows/nix-ci.yml) evaluates those outputs, builds
the Python suite, selected packages and the installed system. These definitions
are not evidence of a passing run or physical hardware qualification. Firmware
and Secure Boot qualification remain separate.

## History

The Debian 13 choice dated 2026-10-24 served the earlier prototype, with a staged
rootfs, package inventory and unsigned VM image; [archived STATUS](archive/STATUS.md)
retains its results. WrOS rebased the operating-system layer on NixOS in rc14.
The retained [Debian builder](../os/scripts/bootstrap-rootfs.sh) requires an
explicit VM-demo mode and is historical test machinery.
