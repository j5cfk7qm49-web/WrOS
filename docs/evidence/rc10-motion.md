# RC10 writing and lock transitions

TachyonFX interpolates foreground colours on the first writing frame and on
tutorial highlights. These effects use the normal input loop and end after
210 ms and 150 ms respectively. Console and low-power frames are static.
After writing input retires, the 150 ms lock transition draws only a
**Locking…** card. Debian's independent locker then handles authentication.

The three motion tests failed against the static implementation and passed
in both the console build and the combined graphical build. They cover
intermediate colour, exact palette restoration, unchanged glyphs and backgrounds,
low-power cancellation and tutorial selection changes. Format, strict Clippy,
all four driven tests and the graphical build passed. The logs are under
`/home/builder/rc10-motion-*.log`.

The installed VMs displayed the first lock and password prompts, rejected a
wrong password, returned to the same writer across three locks, displayed the
lock after wake, and passed credential scans. The native run also exercised
VT switching, reader reacquisition and recovery after reader death. These are
disposable VMs with generated credentials and no NIC. Final release and warmed
HP Stream checks remain pending. [Lock comparison](rc10-lock-visibility.md)

| Renderer | Seconds | Passing run |
| --- | ---: | --- |
| drm | 121.1 | `/home/builder/rc-vm-runs/graphical-drm-20261108T235823Z` |
| foot | 103.5 | `/home/builder/rc-vm-runs/graphical-foot-20261109T000610Z` |
| wgpu | 108.2 | `/home/builder/rc-vm-runs/graphical-wgpu-20261109T000754Z` |

The first foot run, `graphical-foot-20261109T000024Z`, passed three lock
cycles. Its suspend worker then refused the handoff and the writer returned
to login. The diagnostic run `graphical-foot-20261109T000353Z` passed but
used a helper with extra diagnostics. The table counts runs with the
unchanged lock helper. [Foot suspend report #108](https://github.com/j5cfk7qm49-web/WrOS/issues/108)

The tested writer SHA-256 is `1fcb448e1467e1bcd98b473a25fafa580d90a37e86304ad5a755023ac744a007`. The
SHA-256 of the 11 relevant source files, sorted and encoded as
`path + NUL + bytes + NUL`, is `55e01305440b7fc812d4c9edf6cd3bb1fad4e98678599712528475fb4c8eaa89`.
The evidence collector compared those files byte for byte with the Debian
build tree.
