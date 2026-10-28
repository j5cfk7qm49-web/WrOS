# WrOS continuous integration

WrOS is the supported platform. The active [CI workflow](../.github/workflows/ci.yml)
runs in Debian 13 amd64 with the pinned Rust toolchain and CPython 3.13. It
qualifies the Rust workspace in both audio configurations, Clippy, Python
integrations, compiled terminal interactions and the OS static/documentation
checks through `tools/test-m7b.sh`. Results and logs are retained per tested
commit. GitHub's Ubuntu runner hosts the Debian container; it is not a second
WrOS target.

The installer component is also built from pinned Debian source and tested as
a normal user: real native TUI questions plus malformed/disconnected protocol
cases on private noexec-compatible tmpfs. Its private configuration path requires
no privileged test mount or disk. Source downloads happen during provisioning,
and component qualification is mandatory in the offline gate sequence. See
the [installer failure boundary](INSTALLER_BACKEND.md).

Windows and macOS jobs are removed. The imported Grimoire application release,
package and installer workflows are retained as attributed source references in
[upstream-workflows](upstream-workflows/README.md), outside GitHub's executable
workflow directory. Their application installers are not WrOS installers.

Past macOS/Windows results remain historical evidence for their tested commits.
They are not WrOS acceptance requirements. Debian software CI also does not
replace the separate disposable guest tests for boot, PAM, keyboard recovery and
installation, or the later authorized physical hardware qualification. WrOS
image signing, recovery and release qualification remain unfinished under #5
and #13; no upstream publisher is enabled as a substitute.
