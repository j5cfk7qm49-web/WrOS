# Retained terminal library

WrOS uses Crossterm 0.29.0 with its original parser and public API. The unpacked
source is retained under `crates/crossterm-upstream` with its MIT notice. The
adjacent manifest records the original archive checksum, per-file hashes and
explicit patch. The software gate verifies the retained source bytes and
reverses the patch to recover the pinned originals offline.

The Linux Mio source must remember readable input after yielding one parsed
key. Otherwise a multi-chunk burst can leave bytes unread while waiting for an
edge already consumed. The patch remembers that readiness, checks the current
byte count before reading blocking stdin, preserves descriptor flags, and
reports closed input/read errors. It retains the existing bounded read buffer
and parser. A second one-line upstream change removes redundant parentheses
that the current compiler warns about for a local dependency.

This is a retained WrOS patch, not an upstream release or a new input parser.
Normal startup, credential boundaries, reader retirement and protected console
handoffs still require their own real PTY/guest qualification. This does not
complete live installation, lock presentation or suspend/resume support.
