# Installer backend failure boundary

The native installer must stop when its interface disconnects or rejects a
question. This is a prerequisite for the privileged live launcher; the launcher
is still unimplemented and no disk-writing permission is installed.

Actual Debian cdebconf 0.280 tests exposed a problem in the passthrough module:
its GO method ignores most frontend errors and can retain the backend's original
default answer. A half-closed reply stream, malformed status or explicit error
could therefore reach the synthetic confirmation marker. The earlier module is
retained as a failing baseline, never relabeled as passing.

## Maintained component patch

The small retained cdebconf patch now checks writes and flushes, reads one bounded
64 KiB response through standard stdio, and aborts on EOF, an incomplete or
oversized frame, embedded NUL/CR, malformed status or an error response. Status
30 is accepted only for GO with the existing backup capability. Password spaces
are preserved. Credential-bearing developer formats are compiled out of this
module, independently of the rest of cdebconf's build settings.

Partitioning, LUKS, account creation, filesystem copying and bootloader engines
remain Debian components. This patch changes only frontend protocol handling.
It does not replace supervised process lifetime: after any backend or frontend
death the future launcher must stop the entire installer process group, clean
its private mounts and return a truthful interrupted/failed state. It must not
claim that cancellation reverses disk changes already explicitly authorized.

## Reproducible qualification

`os/scripts/build-installer-frontend.sh ... --test-runtime` builds the maintained
backend and its RFC822 database plugin from the same hash-pinned Debian source,
with a private configuration path. The default build still produces the module
only. Both retain source, notices and compiler/build hashes.

`tools/test-installer-components.py` runs the actual native TUI and the protocol
fault corpus as a normal user. Configuration stays inside that private build;
synthetic credentials and databases live in a new mode-0700 tmpfs directory.
The scripts use Debian's template-loading API through `/bin/sh`, so noexec tmpfs
works. The tests need no system configuration bind, root account or target disk.

The corpus includes real Yes/No/Back controls, half-close, truncated response,
invalid/suffixed/negative/error status, NUL, CRLF, oversized response, failed
write with SIGPIPE ignored and an unexpected Back during metadata. It observes
the loaded module, requires failure before GET for rejected confirmations and
checks that no synthetic write marker or lingering config script survives.
The separate native TUI fixture retains multiline explanations, translated
canonical choices, progress, hidden/password-backslash/space/long-value coverage.

Debian CI builds this component and runs both fixture sets as required offline
software gates. It fetches the pinned source through authenticated Debian APT
during provisioning; tests do not download anything. A source descriptor and a
Linux build directory may be supplied through `WROS_INSTALLER_SOURCE_DESCRIPTOR`
and `WROS_INSTALLER_BUILD_ROOT` for an existing isolated developer builder.

Actual live-image/module ABI, matching kernels, root supervision, disk review,
interruption, LUKS and authenticated installation/reboot still need fresh
no-NIC guest qualification before any physical installation test.

The [full local software run](evidence/installer-closed-local.json) passed 20
gates, including both actual component fixtures. The
[source/build/capture record](evidence/installer-closed-local-provenance.json)
links the 12 native checks, 14 protocol cases and packaged-backend compatibility
check. The [old module baseline](evidence/installer-faults-baseline.json) and
[first full-run process-observer failure](evidence/installer-closed-attempts.json)
remain failed. No large unpaced keyboard-paste or live-install coverage is
inferred from the long prefilled-password check.
