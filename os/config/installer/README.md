# Debian cdebconf passthrough framing

The WrOS native installer frontend uses the cdebconf passthrough module from
Debian source version 0.280, with the adjacent small field-framing patch. The
module keeps its original BSD notice. The build retains that notice, upstream
source and Debian's complete copyright index alongside a hash manifest.

The unmodified module failed an actual synthetic multi-paragraph question:
literal newlines entered the line-oriented protocol. Its description code
escaped literal `\n` sequences rather than newline characters, and its progress,
title, substitution and default-value paths also lacked consistent escaping.
The patch escapes backslashes, then newlines, on those outbound fields. The
native frontend decodes those fields once. GET responses contain single-line
answers unchanged, as required by the module's existing answer handling.
Response reading uses a bounded stdio frame and strips line endings without
trimming meaningful password whitespace. The actual fixture covers a prefilled
password spanning several stdio chunks with
literal backslashes. A separate typed password verifies leading and trailing
spaces. Debian's confmodule SET command trims boundary whitespace; shipping
media must not preseed passwords or rely on a prefilled password value.

It aborts on broken transport, incomplete/oversized frames, embedded NUL/CR and
invalid/error statuses. Back is allowed only for GO with
backup enabled. Module developer logging is compiled out locally, so compiling
the full maintained backend still works. See the [failure boundary](../../../docs/INSTALLER_BACKEND.md).

Build non-root with `os/scripts/build-installer-frontend.sh`, passing the pinned
Debian source descriptor and a new output directory. Fetch the source descriptor
and sibling archive through authenticated APT first. The script checks their
hashes before extraction; the descriptor's maintainer signature has not been
independently qualified. This does not replace Debian disk or account engines.

The module is not yet staged into shipping media. A production live launcher
must require this exact compatible module, supervise backend death/disconnection,
keep answers in private RAM, and qualify the full install/reboot path in new
guests before exposing disk-writing actions.

The optional `--test-runtime` build adds a private configured backend and RFC822
plugin for ordinary non-root CI. `tools/test-installer-components.py` uses it
without binding or changing system configuration; synthetic credential files
stay on tmpfs, including when mounted noexec.
