# M7b prose, dictionary, voice and revision tools

WrOS 0.1.0~rc16 packages the prose-tools runtime through
[nixos/pkgs/prose-runtime.nix](../nixos/pkgs/prose-runtime.nix). The services
below remain part of the portable writing core; the installed runtime and tool
paths come from the NixOS build.

## Implemented scope

- [prose.rs](../crates/grimoire-core/src/prose.rs) provides local readability,
  sentence/paragraph patterns, repetition and fiction presets. Readability is
  withheld below 100 analysis words and remains advisory.
- [prose_tools.rs](../crates/grimoire-core/src/prose_tools.rs) and the
  [Python adapter](../crates/grimoire-core/assets/wros/prose_tools.py) provide
  exact-version proselint and optional Nabokov/spaCy analysis. Nabokov requires
  an explicit, single-use permit bound to the source and configuration. Opening
  a project or dashboard does not grant that permit. Results must match the source
  and configuration before use.
- [lexicon.rs](../crates/grimoire-core/src/lexicon.rs) reads local WordNet 3.0
  senses and explicit antonym pointers without model or network access.
- [voice_profile.rs](../crates/grimoire-core/src/voice_profile.rs) stores
  author-selected references, hashes and numeric measurements. A reference
  profile needs 5,000 analysis words, 40 segments and 150 distinct words;
  comparison needs 300 words. The limits are eight profiles and 64 references
  per profile. Profiles do not copy reference prose or detect authorship.
- [review_queue.rs](../crates/grimoire-core/src/review_queue.rs) provides
  source-bound review receipts and a ranked advisory queue. Equivalent findings
  retain engine provenance; priority is a review order, not a quality score.

The analyzer source cap is 65,536 bytes. Optional Python workers run for explicit
requests rather than remaining resident. Their CPU limit is 30 seconds with a
32-second hard limit; the Rust parent applies a separate 60-second wall budget.
Failures, timeouts and unavailable dependencies are distinct from a completed
analysis. Native editing and spelling remain independent of optional models.
These boundaries are defined in the adapters linked above.

## Target environment and reproduction

The [shared lock](python-tools-lock.json) targets Linux x86_64 and CPython 3.13.
[prose-runtime.nix](../nixos/pkgs/prose-runtime.nix) fetches the declared artifacts
by hash and runs the provisioner offline. The installed payload uses the pinned
Nixpkgs interpreter through its packaged `bin/python3`.
[base.nix](../nixos/modules/base.nix) sets `WROS_TOOLS_PYTHON` to that runtime;
[paths.nix](../nixos/pkgs/paths.nix) also supplies it as a compiled tool path.
The retained Debian build uses `/usr/bin/python3`.

Build the current runtime and NixOS Rust suite on a Nix build host:

```sh
nix build .#wros-prose-runtime .#checks.x86_64-linux.rust-suite
```

[rust-packages.nix](../nixos/pkgs/rust-packages.nix) defines the Rust suite with
the packaged runtime, WordNet and tool paths, workspace Clippy and workspace
tests. [flake.nix](../flake.nix) exposes it as a check. The
[Nix workflow](../.github/workflows/nix-ci.yml) evaluates every check, runs the
Rust suite in both audio configurations and the app suites, and builds selected WrOS
packages and the installed system; it does not run the VM checks. The Debian workflow that
ran `tools/test-m7b.sh` was retired in rc17; the script still runs in Debian 13.
A workflow definition is not a passing-run record.

Retained notices and model declarations are in
[python-tool-notices.json](python-tool-notices.json),
[prose-model-licenses.json](prose-model-licenses.json) and
[lexicon-licenses.json](lexicon-licenses.json). Hashes and declarations identify
the packaged inputs; independent corpus review remains separate.

## Remaining acceptance boundaries

The encrypted installer, authenticated login, independent lock and signed
generation updates/recovery are implemented in the
[installer core](../nixos/scripts/wros-install-core),
[session module](../nixos/modules/session.nix),
[lock handoff](../src/console_lock.rs) and
[update engine](../nixos/scripts/wros-update-engine.py).
[Continuity](../crates/grimoire-core/src/continuity.rs),
[milestones and variants](../crates/grimoire-core/src/milestones.rs),
[read-through queues](../crates/grimoire-core/src/speech.rs),
[footnotes and export checks](../crates/grimoire-core/src/preflight.rs) and
encrypted backups are also implemented. The
[boot/login VM check](../nixos/tests/boot-unlock-login.nix) includes a local
backup and restore comparison. Current NixOS and physical qualification must be
assessed separately from historical M7b results; a test definition is not a pass.

## History

The Debian-era M7b qualification on `codex/m7b-qualification` / PR #15 is recorded
under 2026-10-26 in [archived STATUS](archive/STATUS.md). It retains software,
UI/PTY and guest results, including the emulated trained-worker CPU-denial
boundary. Those totals and measurements do not qualify the current NixOS
runtime or physical hardware.
