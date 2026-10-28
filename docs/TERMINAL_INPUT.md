# Terminal input readiness

The native installer exposed a locked Crossterm 0.29.0 Mio behavior: a raw burst
could consume the first 1024 bytes, leave the remainder in the terminal and wait
for an edge already consumed. A fresh private-PTY probe accepted 40, 200 and
1000 bytes but timed out for 3000. The stalled process still had 1977 unread
terminal bytes; another Enter did not complete the field.

WrOS keeps the maintained parser/API and retains the exact checksum-verified
upstream crate with its MIT notice. The
[explicit patch and source manifest](../os/config/terminal/README.md) remember
readability across returned events and check available bytes before continuing
a blocking stdin read. Descriptor flags stay unchanged. Closed input and read
errors terminate the reader. The existing read-buffer size and parser remain;
this does not introduce a separate keyboard parser.

The first prototype assumed nonblocking stdin and failed even short entries;
that failure remains separate. The corrected private probe accepted the same
3000-byte burst in roughly 3 milliseconds. The later
[full local run](evidence/terminal-input-local.json) passed 972 Rust/configuration,
124 Python/zero skips, both Clippys and 21 gates. The
[actual no-NIC guest](evidence/terminal-input-vm.json) passed in 228.09 seconds
with unchanged base image/source, exact new binaries, real PAM/lock/reader
handoffs and process-death recovery. [Source and artifact provenance](evidence/terminal-input-local-provenance.json)
and [original exploratory failures](evidence/terminal-input-observations.json)
are separate. Debian CI is pending; neither scope certifies a live installation.

Regression coverage exercises ASCII and Unicode across read boundaries,
unchanged descriptor flags, reader retirement/backpressure and fresh parsers,
hidden long passwords at four terminal sizes, and discarding post-submit
typeahead before another confirmation. The real cdebconf fixture also types a
long password with boundary spaces and literal backslashes. Raw keyboard bursts
and bracketed-paste support remain distinct contracts.

All normal WrOS surfaces keep the shared Grimoire presentation. Input-library
qualification does not complete native lock presentation, privileged live
launch, complete media, LUKS installation or lock-before-suspend/resume.
