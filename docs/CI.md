# WrOS continuous integration

WrOS has two active software workflows. [Nix CI](../.github/workflows/nix-ci.yml)
evaluates every package and check, runs the Nix Python suite, and builds the WrOS
packages and installed system from the pinned flake. VM checks, the ISO and the
Rust-suite check are executed separately in local qualification. The retained
[Debian CI](../.github/workflows/ci.yml) runs M7b and driven terminal/OpenGL checks
in Debian 13; its results qualify that environment.

The following installer-component, terminal-library verifier and matching
kernel/initrd paragraphs describe the retained Debian workflow. The current
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
