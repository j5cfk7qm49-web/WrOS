# Power-action safety guards

WrOS **does not offer Suspend** until a separate secure screen locker and
lock-before-suspend/input-ownership protocol have been qualified. Both native
Power and retained System settings say why. The Linux adapter independently
rejects `Request::Power(Suspend)` before invoking any external process. The
simulator also refuses it. A manually constructed legacy confirmation cannot
bypass the controller guard. This addresses issue #32; it is not a session lock.

Shutdown and Restart still start with Cancel selected. A confirmed action first
attempts to save both document buffers, then requires **no dirty scene, no stale
tree and no unresolved source conflict**. Otherwise the confirmation remains
open with a visible refusal, and no worker request is sent. Return to writing to
review conflicts or unavailable storage; retry is explicit after recovery.

## Why the caller needs a stricter gate

The existing general `commit_saves()` intentionally treats a temporary missing
source as waiting after recovery-copy attempts, and preserves external changes
by parking original edits. It can return true while buffers are still dirty or
new conflict files await a tree reload. A normal save can therefore preserve
words successfully without satisfying a power request's requirement that every
buffer is durably saved and its conflicts reviewed. Its broader semantics are
unchanged. The Power caller now checks those extra conditions; `tree_stale`
also catches a freshly parked copy before it has a row in `conflicts()`.

The source-conflict regression initially reproduced a Restart request through
the recorded simulator despite unresolved source changes. Checking the current
conflict list alone also failed because the newly parked file was not indexed
yet. Both original and external contents were preserved in those fixtures; no
real power operation occurred. Issue #34 tracks the stricter caller contract.

## Verification and remaining work

The focused offline suite exercises the actual Linux adapter with a recording
runner, both native and legacy controls, a forged old confirmation, retained
two-pane position/selection/focus/undo at four sizes, default cancellation,
normal confirmed saves and source-conflict refusal. Missing source and genuine
nonregular-file document-claim failures preserve editor text, backing bytes and
undo; restoring the backing file allows an explicit retry. The actual compiled
four-size Settings PTY also enters the unavailable Suspend row, tests paste
ownership and returns to the original caret/save/undo/restart flow. Both Clippy
configurations and ordinary documentation gates are required.

The [executed focused record](evidence/power-guard-targeted.json) passed **17
backend tests, 11 Settings/controller tests, one existing asynchronous System
test**, both Clippys, the actual four-size Settings PTY and documentation/diff
checks, inside a network namespace without external connectivity. Its source
fingerprint is `56e59e1b858f2da7c2366e9fe88c9f5d712cce3515de033c5e6aa5fb02058410`.
The [separate full CI record](evidence/power-guard-ci.json) from Debian/macOS run
[40468810112](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40468810112)
passed **932 Rust tests/configuration** (5 default / 4 no-audio ignored), both
Clippys, **96 Python methods / zero skips** and **17 CI gates**. Code head is
`15512d4`; CI tested merge revision `e656c3b0cdfccd282f5ea536bafa45b730da1634`.
Its source fingerprint matches the focused record. CI has no staged image, so
it does not claim the local verifier's additional Wi-Fi image audit. The
separate upstream Windows check was still running when recorded.

No test suspends or powers off the host. Controlled power cases use a recorded
simulator and the Linux no-process guard. New full CI is tracked separately from
the earlier warm-screen source/binary/VM evidence.

Issues #4/#27 still require isolated PAM/greetd login, secure locking, exclusive
input/VT ownership, acknowledged lock before sleep, crash/logout/account-switch
handling and physical suspend/resume qualification. This application guard does
not configure or qualify logind lid/power-key behavior or prevent a user from
requesting sleep through other Linux tools. No production session claim is made.
