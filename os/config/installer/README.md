# Debian cdebconf passthrough framing

The matching backend recipe explicitly includes maintained `apt-cdrom-setup`.
Its presence in the offline pool alone does not select it: `apt-setup-udeb` can
load the real mirror provider instead of the CD provider's virtual alternative.
The build checks the actual `40cdrom` generator in the compressed initrd, and
the native fixture refuses to start its frontend when that provider is absent.

`native-crypto-passphrase` is a fixed-version partman init hook for the native
passphrase path. Debian partman-crypto 134 unconditionally requests a frontend
entropy plugin, including for passphrases. Debian does not provide a passthrough
entropy plugin. The hook accepts only the authenticated script's exact hash and
removes that unused plugin request. It leaves maintained cryptsetup, kernel
randomness and `blockdev-keygen` unchanged. No `plugin-entropy` capability is
advertised; Debian's existing check rejects random-key requests. The stock text
frontend keeps its entropy plugin. This adapter is staged by the guarded VM
fixture; production media integration and full installation remain unqualified.

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

`build-live-backend.sh` can now add this module as a local udeb to a matching
Debian Security kernel/initrd, using retained upstream build machinery and an
explicit initrd-only dependency profile. See the [component recipe and actual
image backend evidence](../../../docs/LIVE_INSTALLER_IMAGE.md). This does not
expose a privileged launcher or complete a live ISO.
