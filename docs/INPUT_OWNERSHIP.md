# Editor input ownership prerequisite

The Linux editor now reads terminal events through an unprivileged child
process. It re-executes the same installed binary in a private mode before any
project, configuration, rescue or `App` initialization. There is no companion
executable to find or install. The independent MIT `wros-input` crate has no
editor, manuscript, authentication or system-service dependency.

The parser needs a process boundary because crossterm can retain an incomplete
escape sequence or bracketed paste internally. Pausing a thread and draining
complete events cannot discard that private partial state. See crossterm's
[read/poll contract](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html).
The child keeps the existing blocking read while idle, without a polling loop.

## Handoff contract

`Input::pause()` first stops delivery and discards its bounded pending queue,
then kills and reaps the reader, joins its pipe-decoding thread and flushes
kernel terminal typeahead. Only then does it return an opaque, single-use
`Paused` proof. Killing the process discards its complete and partial parser
state, including an unfinished paste. Failure produces no proof and no automatic
restart. Dropping the proof keeps input paused.

An independent terminal owner must finish and exit before
`Input::resume(proof)`. Resume verifies the owner/epoch, flushes kernel typeahead
again, and starts a fresh parser process with an acknowledged startup. A bad
proof, failed flush or failed startup cannot resume editing. The controller
stays on its original thread, enforced by the type, because Linux's parent-death
signal is attached to the spawning thread.

The worker verifies its parent after arming `PR_SET_PDEATHSIG(SIGKILL)`, so an
editor crash cannot leave an orphan reader consuming a subsequent login. It
refuses root, disables core dumps/dumpability, uses fixed environment and argv,
and owns no manuscript state. Queue payloads and framing buffers are wiped
when discarded. This does not claim resistance to privileged memory access or
all allocator copies.

The private pipe uses bounded little-endian frames with a 2 MiB limit checked
before body allocation and a 32-event channel with backpressure. Startup
version/PID/UID are checked. Malformed/disconnected/oversized input yields fixed
errors without raw event contents. An oversized paste is rejected; prior edits
are saved before the editor exits. Ordinary reader failure follows the existing
save/rescue path for both buffers. No typing is silently truncated into a
different paste.

## Scope and acceptance

This supplies the editor's reader-retirement primitive. **It does not lock the
screen, authenticate an unlock, grant exclusive seat/VT ownership, or authorize
suspend.** A credential prompt additionally requires a maintained independent
locker with protected terminal ownership. That integration and an authoritative
lock-ready acknowledgment before sleep remain issue #4. No lock command or
password prompt is added to the editor; Suspend remains unavailable.

The future controller must also stop editor rendering and all terminal/cursor
queries while another owner is active. The proof describes this input reader's
retirement; it does not manage Ratatui or prevent unrelated processes from
opening a terminal. A background locker started while the normal editor loop
continues is not a completed handoff.

Tests use new unprivileged private PTYs, never the host terminal. The standalone
ownership fixture exercises a partial paste already consumed by the old parser,
reader reaping, a distinct test reader, stale typeahead on return, a full queue,
proof drop, editor-parent SIGKILL, and flush/startup failure. It simulates input
ownership, not authentication. Actual editor PTYs verify both dirty buffers are
saved on reader death, parent death leaves no executable reader, and private
reader mode does not create a project in an invalid context. Existing four-size
settings/cursor/undo/restart tests cover normal event routing.

[Focused software qualification](evidence/input-ownership-focused.json) passed
four unit tests, one thread-affinity compile-fail test and four real private-PTY
ownership scenarios in both configurations, both workspace Clippys/build,
three actual editor fault/context tests and the retained four-size Settings
PTY. All 11 checks passed with external networking isolated. Source is
`0f668f5d8bb5e757138060d0321c2bd50bfa5ba161b2655bac8e1ad5f8e17aba` / 304 files.
The [native-login guest](evidence/input-ownership-vm.json) passed in 376.22 seconds
with no NICs and an unchanged base image. It verified the actual UID-1000/1001
reader processes, abrupt writer exit and ordinary logout without orphan readers,
PAM rejection/success, hidden input, account switching, all four keyboard layouts
and the journal/home secret scan. Focused and guest editor/greeter binaries match
the archived fixture exactly. This is session integration, not a VT-lock/unlock test.

Fresh [Debian CI 40601108193](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40601108193)
passed implementation `930ee9e`: [959 Rust tests per configuration, both Clippys,
99 Python tests/zero skips and 17 checks](evidence/input-ownership-ci.json). The
four standalone ownership scenarios also ran in each CI configuration. Its
source fingerprint matches the focused and guest records; tested merge is
`25c2b9fd1b7b132f5f966c94c47fe9e6a0b30f95`. CI binaries are recorded independently.
Previous keyboard/login evidence retains its original source.
