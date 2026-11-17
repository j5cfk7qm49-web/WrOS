# WrOS continuous integration

WrOS has one software workflow. [Nix CI](../.github/workflows/nix-ci.yml)
evaluates every package and check, runs the Rust workspace tests in both audio configurations, the driven app tests (terminal and an OpenGL window under headless Cage), the full Python app suite and the repository gates
(`tools/nix-gates.sh`), and builds the WrOS packages and installed system from the
pinned flake. Each test run is a flake check (`rust-suite`, `rust-suite-no-audio`,
`rust-driven-suite`, `app-suite`, `python-suite`), so `nix build` runs the same thing
locally. [tools/nix-python-suite.py](../tools/nix-python-suite.py) lists the
Debian-only Python tests NixOS does not run, each with its reason. VM checks and the
ISO run in local qualification. The Debian workflow (`ci.yml`, M7b in Debian 13) was
retired in rc17, once Nix CI ran the same Rust and Python tests with the same counts; its
runs up to rc17 remain in the repository's Actions history.

The following installer-component, terminal-library verifier and matching
kernel/initrd paragraphs describe the retired Debian workflow (`tools/test-m7b.sh`, still runnable in Debian 13). The current
NixOS installer uses `nixos/scripts/wros-install-session`, `wros-installer-backend`
and `wros-install-core`; its tests and build coverage are separate.

The installer component is also built from pinned Debian source and tested as
a normal user: real native TUI questions plus malformed/disconnected protocol
cases on private noexec-compatible tmpfs. Its private configuration path requires
no privileged test mount or disk. Source downloads happen during provisioning,
and component qualification is mandatory in the offline gate sequence. See
the [installer failure boundary](archive/INSTALLER_BACKEND.md).

The retained terminal-library gate checks all original upstream file hashes,
the explicit local patch and offline recovery of the originals. Actual
multi-read input, Unicode, unchanged descriptor flags and confirmation
typeahead are covered separately by the reader/installer PTY regressions.

The [matching kernel/initrd recipe](archive/LIVE_INSTALLER_IMAGE.md) has separate local
artifact and actual-image backend qualification. Standard software CI checks its
shell syntax and the shared drivers; it does not build or certify a complete
live ISO or implicitly repeat that archive-backed initrd build.

Windows and macOS jobs are removed. The imported Grimoire application release,
package and installer workflows are retained as attributed source references in
[upstream-workflows](upstream-workflows/README.md), outside GitHub's executable
workflow directory. Their application installers are not WrOS installers.

Past macOS/Windows results remain historical evidence for their tested commits.
They are not WrOS acceptance requirements. Debian software CI also does not
replace the separate disposable guest tests for boot, PAM, keyboard recovery and
installation, or the later authorized physical hardware qualification. Signed
NixOS update bundles, channel publication and authenticated recovery are
implemented. Run the candidate's local qualification and record hardware results
separately. Do not infer ISO-signing or Secure Boot qualification from
update-bundle signatures. No upstream application publisher substitutes for WrOS
release qualification.
