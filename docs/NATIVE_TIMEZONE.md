# Native timezone selection

Settings → System → Date and time → Choose timezone loads the local systemd
timezone catalog. Type a city/region to filter it, use arrows to select, then
Enter. The confirmation defaults to Cancel. A failed/denied request retains the
choice for an explicit retry; it never reports success or changes cached status.
Leaving Settings returns to the same editor, caret, selection and undo history.

The bounded system worker calls Debian's maintained
[timedatectl interface](https://manpages.debian.org/trixie/systemd/timedatectl.1.en.html)
with fixed argv, no shell, null stdin and `--no-ask-password --no-pager`.
Identifiers and the entire catalog are validated before use. Setting a timezone
must be followed by a matching authoritative `Timezone` readback. Status also
reads timedated: `/etc/timezone` can remain stale when `/etc/localtime` changes.
Errors/mismatched readback keep writing available and require refresh/retry.

Only the active local `writingdeck` group receives the existing
`org.freedesktop.timedate1.set-timezone` polkit action. There is no root editor,
custom privileged helper, password prompt or grant for setting the clock, RTC,
NTP, arbitrary commands or general administration. Those date/time controls are
still unavailable. No network is required to select an installed timezone.

Timezone changes affect future local dates; historical writing records retain
their recorded dates/offsets. The existing tutorial's SystemOpened event proves
navigation only, not that a timezone was changed. Offline help includes an
optional cancel-first exercise. Software, actual guest/policy and image evidence
are separate gates. In particular, the initial integrated native-login ISO built
at `9e7d99c` predates these controls and is not evidence for this increment.

[Focused verification](evidence/native-timezone-focused.json) passed all ten
offline gates: 20 system-backend tests, the new controller scenario at four
sizes, workspace Clippy/build, the retained Settings/cursor/undo/restart PTY at
four sizes and static checks. The [real no-NIC guest](evidence/native-timezone-vm.json)
passed in 480.58 seconds, including native search, Cancel as the default,
explicit application and authoritative readback of Asia/Tokyo, inactive-session
and unrelated-action denial, plus retained PAM/keyboard/account/input/crash and
secret-scan checks. The base image stayed unchanged. Source and the exact
editor/greeter binaries match the archived guest fixture. Full Debian CI remains
pending; this does not qualify a new ISO, manual clock control or physical hardware.
