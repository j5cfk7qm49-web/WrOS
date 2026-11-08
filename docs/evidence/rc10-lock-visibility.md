# RC9 and RC10 lock display comparison

The owner reported a black screen around locking in RC9. A switch between tty1
and tty2 brought back the picture with warmth visible. The stage at which the
display went black remains unconfirmed. The RC9 locker switches to tty8 and
runs Debian vlock after retiring the writing reader. It makes no warmth request.
The older report involved DRM ownership during the Intel display driver's boot
takeover. The shared VT recovery suggests a display transition but does not
identify the cause. [Lock report #107](https://github.com/j5cfk7qm49-web/WrOS/issues/107)
· [Boot warmth report #30](https://github.com/j5cfk7qm49-web/WrOS/issues/30)

The saved RC9 package on a disposable overlay reached the first lock, showed
its password prompt and returned to writing without a VT switch. The native
RC10 clock build showed those three stages and the lock after suspend/wake.
The harness checks the console contents and the captured pixels separately.
In the RC10 run the intentional maintenance VT test follows the lock cycles.
Both VM runs passed their credential scans. [RC9 lock](rc9-lock-visible.png)
· [RC10 lock](rc10-lock-visible.png)

| Installed VM | Result | Seconds | Run directory |
| --- | --- | ---: | --- |
| `0.1.0~rc9` package | Passed | 60.6 | `/home/builder/rc-vm-runs/rc9-lock-std-20261108T234449Z` |
| RC10 native clock checkpoint | Passed | 120.7 | `/home/builder/rc-vm-runs/graphical-drm-20261108T234550Z` |

Both Bochs display runs reported that an active output lacked supported gamma
controls. The warmed HP Stream lock remains unverified. The owner can compare
the saved warm choice with Off at the first lock and on wake. [Hardware checklist](../HARDWARE_CHECKLIST.md)

The first RC9 screenshot text check timed out even though the prompt was
legible. The installed console uses a different font from the screenshot
reader's template. The new contrast check rejects uniform frames and frames
with only a cursor. All 21 harness unit tests passed. The failed text-reader
run remains under `/home/builder/rc-vm-runs/rc9-lock-std-20261108T234114Z`.
