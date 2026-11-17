# Native timezone selection

Settings → System → Date and time → Choose timezone loads the local systemd
timezone catalog. Type a city/region to filter it, use arrows to select, then
Enter. The confirmation defaults to Cancel. A failed/denied request retains the
choice for an explicit retry; it never reports success or changes cached status.
Leaving Settings returns to the same editor, caret, selection and undo history.

The bounded system worker calls systemd's maintained
[timedatectl interface](https://manpages.debian.org/trixie/systemd/timedatectl.1.en.html)
with fixed argv, no shell, null stdin and `--no-ask-password --no-pager`.
Identifiers and the entire catalog are validated before use. Setting a timezone
must be followed by a matching authoritative `Timezone` readback. Status also
reads timedated: `/etc/timezone` can remain stale when `/etc/localtime` changes.
Errors/mismatched readback keep writing available and require refresh/retry.

Timezone selection uses `org.freedesktop.timedate1.set-timezone` for active local
members of both `wros` and the `writingdeck` compatibility group, without a custom timezone helper or
password prompt. Manual clock setting and returning to network time are also
available through the separate `wros-clock` helper and its `org.wros.clock`
action. RTC controls and arbitrary commands are not exposed.
Selecting an installed timezone needs no network.

Above the timezone, a row reads `Network time: on · synchronized`, `on · not
yet synchronized`, `off` or `unknown`, from `timedatectl show --property=NTP
--property=NTPSynchronized`. New installs ship `systemd-timesyncd`, so the
clock follows the network once online; a legacy Debian rc1 install can enable
it with (Debian builds only)
`sudo apt install systemd-timesyncd && sudo timedatectl set-ntp true`.

Timezone changes affect future local dates; historical writing records retain
their recorded dates/offsets. The existing tutorial's SystemOpened event proves
navigation only, not that a timezone was changed. Offline help includes an
optional cancel-first exercise. Software, actual guest/policy and image evidence
are separate gates. In particular, the initial integrated native-login ISO built
at `9e7d99c` predates these controls and is not evidence for this increment.

## Debian-era qualification history

These recorded counts, durations, CI runs and guest results predate the NixOS
build and do not qualify its current release.

[Focused verification](evidence/native-timezone-focused.json) passed all ten
offline gates: 20 system-backend tests, the new controller scenario at four
sizes, workspace Clippy/build, the retained Settings/cursor/undo/restart PTY at
four sizes and static checks. The [real no-NIC guest](evidence/native-timezone-vm.json)
passed in 480.58 seconds, including native search, Cancel as the default,
explicit application and authoritative readback of Asia/Tokyo, inactive-session
and unrelated-action denial, plus retained PAM/keyboard/account/input/crash and
secret-scan checks. The base image stayed unchanged. Source and the exact
editor/greeter binaries match the archived guest fixture. [Debian CI 40614744076](https://github.com/j5cfk7qm49-web/WrOS/actions/runs/40614744076)
passed implementation `c88c66c`: [963 Rust/configuration, both Clippys, 108 Python/
zero skips and 17 gates](evidence/native-timezone-ci.json). The four standalone
input-ownership scenarios also passed in each CI configuration. Its source
matches the focused/guest records exactly; tested merge is
`139e2c4dba96dbbc0e4c985469f72c387b34d111`. CI binary identity remains separate
from the local guest binary. This does not qualify a new ISO, manual clock
control or physical hardware.
