# Native installer target review

The [actual no-NIC guest](evidence/native-target-review-vm.json) passed in
**478.14 seconds**. Debian's partitioner reached its LVM disk-write question
through the shared Grimoire TUI, identified virtual disk `vda`, and displayed
No selected. Enter declined the write and returned to the partition overview.
The complete **16 GiB** target hash was identical before and after the run.
Stopping the standard service retired the frontend, backend and private mounts;
the guest then powered off. The original live ISO and capsule also stayed unchanged.

The [write question](evidence/native-target-review.png) and
[returned partition overview](evidence/native-target-declined.png) are actual
QEMU framebuffer captures, with [pixel identities](evidence/native-target-captures.json).
Their source identity is `8128d68d...4fa26` / 427 files. This proves refusal and
containment, not an installed encrypted system or recovery after a write.

The fixture creates its own new fixed-size disk; no argument can nominate an
existing host disk. A fresh full 128-bit nonce fits virtio's 20-byte identifier
using Base85. Root checks the marker, serial, capacity and device number before
allowing that disk, its actual partitions or exclusively derived mapper chains.
Foreign dependencies, cycles, stale identity and unexpected writable devices
are refused. Ordinary diskless operation retains its stricter disk rejection.
Only the fixture has target-selection defaults. Its allowlist rejects commands,
write approvals and unlock keys; shared ISO defaults remain interactive.

The test retires the writing session before starting another input reader. A
bounded wait uses pidfd exit readiness, including exited but unreaped processes;
a live reader still prevents startup. A real child-process regression checks
both states. The installer stays non-root on tty7. The newly compiled frontend
is staged read-only only inside the fixture's private service namespace, and its
observed executable hash is checked at every form. It is not yet in a rebuilt ISO.

The native event loop now waits on its backend channel when the keyboard is
idle, rather than imposing a 15 ms keyboard wait after every protocol message.
Buffered keyboard input also runs without a per-character delay. In the
[retained measurement](evidence/native-target-protocol.json), 500 sequential
passive exchanges fell from **7.8773 to 0.3667 seconds**. This measures transport,
not complete installation time. The actual PTY regression also checks No after
the burst; existing long raw input, hidden passwords and typeahead checks remain.
The first channel-wait attempt broke four long-input subcases and was corrected;
its result remains separate from the accepted executable.

[Original failed VM observations](evidence/native-target-attempts.json) retain
their own source and input identities. These include virtio identifier truncation,
an insufficient component-loading deadline, reader retirement and an expected
no-Ethernet question. No failed observation is relabelled passed. The final
driver waits only for actionable forms, bounds the whole review, and refuses
unrecognized questions without answering them.

The [full local software run](evidence/native-target-local.json) passed **973
Rust/configuration, 162 Python/zero skips, both Clippys and 21 gates**.
[Provenance](evidence/native-target-local-provenance.json) matches the guest source
and the byte-reproduced standalone guest executable. The separately identified
[workspace and standalone binaries](evidence/native-target-executables.json)
retain their own hashes; the full verifier PTYs exercised the workspace build.
[Debian CI 40757537144](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40757537144)
passed 973 Rust/configuration, 162 Python/zero skips, both Clippys and 20 gates.
Its [artifact provenance](evidence/native-target-ci-provenance.json) matches the
local/guest source and verifier inputs. The actual disk-refusal VM remains a
separate local acceptance result. The delivered
[live image](LIVE_SESSION.md) still has its earlier executable. Actual encrypted
installation and authenticated target boot, owned LUKS/LVM interruption cleanup,
the trusted production editor handoff, refreshed media and physical qualification
remain open. See the [backend capsule](NATIVE_INSTALLER_ENGINE.md) for its separate
authenticated package and matching-kernel provenance.
