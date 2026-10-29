# Retained terminal library

WrOS uses the locked Crossterm 0.29.0 parser and public API. The original crate
archive is checksum-verified, retained under `crates/crossterm-upstream`, and
keeps its MIT notice and upstream attribution. The adjacent manifest records
all original files and the explicit patch; the required software gate checks
all bytes and reverses the patch to recover the pinned originals offline.

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
