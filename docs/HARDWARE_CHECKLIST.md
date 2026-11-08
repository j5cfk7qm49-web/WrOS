# HP Stream hardware checklist

The VM chain proves installs, updates and the installed services. These checks
need the real laptop: an HP Stream 11 (Celeron N4000, 4 GB, Intel UHD 600,
RTL8822BE Wi-Fi, 32 GB eMMC). Run them on each release candidate before it
becomes a final release, and after any change to power, Wi-Fi, the console or
the boot.

Use a disposable book for anything that types, locks or suspends. Never test
against the real manuscript. The tutorial's `d` key (F12 › Interactive
tutorial) makes a safe demo book; Open another book returns to yours.

Write the result of each check in the table at the end, with the release, the
date, and anything odd. "Pass" means every line under **Pass when** held.

## 1. Typing and speed

1. Unplug the charger. Open the disposable book and type for ten minutes at
   your normal speed, including fast bursts and held keys.
2. Move between scenes, open the corkboard and Settings, and come back.

**Pass when:** no character is lost, doubled or out of order; the cursor never
lags behind your typing; moving around feels no slower than on AC.

## 2. Lid, lock and sleep

1. On battery, type a sentence in the disposable book and close the lid
   straight away. Wait a minute, then open it.
2. Unlock with your password. Check the sentence.
3. Repeat on AC power.
4. Settings › System › Battery and power › **Lock and suspend**, then wake the
   laptop with the power button and unlock.
5. Settings › System › Battery and power › **Lock writing session**, close the
   lid while it is locked, then open it.
6. Without restarting, close and open the lid twice more.
7. Settings › System › Battery and power › **When the lid closes**: set it to
   *lock, stay awake* and close the lid; then *do nothing* and close the lid.
   Set it back to *lock and sleep*.

**Pass when:** each close saves first (the sentence is there after unlock),
the screen after waking is the lock, never the book or the login screen; the
Colemak password works at the lock; a wrong password is refused; after step 5
the laptop stayed awake and locked; in step 6 every close locks; in step 7 the
first close locks without sleeping and the second leaves the book open. Nothing
you typed at the lock shows up in the book.

## 3. Wi-Fi after sleep, and over a day

1. After each wake in check 2, wait 30 seconds and open Settings › System ›
   Wi-Fi and networking.
2. Use Wi-Fi as normal for a whole day, with the lid closed and opened a few
   times.

**Pass when:** Wi-Fi reconnects by itself after every wake, and a backup or
sync later that day works. At the end of the day, collect the log (below) and
check it has no new `failed to leave lps state` lines.

## 4. School network

1. At school, open Settings › System › Wi-Fi and networking and scan. Pick
   the school network and join.
2. Open a web check that needs the network (Settings › Updates and recovery › Updates ›
   Check for WrOS updates is enough).

**Pass when:** the network appears in the scan, joins, and the update check
reaches the internet. If the school filters by hardware address, joining at
all shows the laptop kept its registered address.

## 5. Low battery

1. Unplug the charger with a disposable book open and keep writing now and
   then (lid open, screen on) until the battery is nearly empty. Note what the
   bottom bar says at 20 %, at 10 % and at 8 %.
2. At 8 % WrOS saves at once and counts down 30 seconds before it locks and
   sleeps. Once, plug in during the countdown, then unplug again.
3. After it sleeps, plug in, wake it, unlock.

**Pass when:** each warning shows once and says what happens at 8 %; plugging in
says "Charging" and stops the countdown; it suspended before the battery died,
never went to the login screen, the last words you typed are in the book, and
the unlock message says the battery made it sleep.

## 6. Shutdown and restart during a backup

1. Start a backup of the disposable book (Settings › Backups and accounts › Backups), and
   while it runs pick Settings › System › Battery and power › **Shutdown**.
2. Do the same with **Restart**, and once more during a Drive sync.

**Pass when:** WrOS says the job is running and waits for it or asks, and never
powers off in the middle of it. The backup listed afterwards verifies.

## 7. Recovery boot

1. Restart. While WrOS starts, press Esc.
2. In the recovery menu, choose **Show what happened**, read it, go back.
3. Choose **Continue to the normal login**.

**Pass when:** the menu appears, both choices work, and the normal login and
book open afterwards.

## 8. After an update

Right after a release candidate installs on the laptop:

- Your settings carried over: warm screen, keyboard layout (Colemak at the
  login, the lock and in the book), Wi-Fi networks, Drive link.
- The warm screen comes on at boot with no black screen.
- Settings › Updates and recovery › Updates shows the new version, and offers to go back to
  the previous one.

## 9. Sound

1. Open Settings › System › Sound and volume. Play a sound if you have one
   (or just watch the numbers).
2. Choose Volume +10%, Volume -10%, then Mute and Unmute.

**Pass when:** the page shows the volume, each choice changes it, and mute
silences the speakers.

## 10. Boot screen

1. In Settings › System › Display, choose **Start screen: WrOS**. Wait until
   preparation has finished (refresh local system status to check), then restart.
   Watch from the boot menu to the login screen.
2. At the disk password, type it in Colemak and press Enter. Once, type it
   wrong first.
3. Restart and press Esc at the disk password. Unlock at Debian's text prompt.
4. Restart, press Esc to show the boot menu, and choose **WrOS (text start)**.
   Unlock in Colemak and sign in as usual. Check the recovery entry too.
5. Choose a different preset in Settings › Look and keyboard › Look › Themes.
   Wait for preparation, restart, and check its accent and border on the unlock,
   progress and login logos. Repeat with a custom theme.
6. Choose **Start screen: plain text**, wait for preparation, and restart.

**Pass when:** only the WrOS logo and the loading bar show; the password prompt
shows under the logo with one dot per key; the Colemak password unlocks and a
wrong one asks again; Esc and text start reach Debian's prompt; the progress
screen ends before the login and its logo sits above the complete form. The
chosen colours reach all three logos. Plain text restores Debian's prompt.
Record the restart and unlock times on the Celeron. If the Colemak password is
refused at the WrOS prompt, press Esc and type it at the text prompt, then report it.

## 11. Native graphical defaults (rc10)

On the HP Stream, check that disk unlock, loading, login and writing all use the
smooth native font and the chosen colours. A clean or updated system should do
this without enabling a preview. Retest Colemak, accented letters and compose in
writing; wrong and correct disk/login passwords; Esc at disk unlock; and the
**WrOS (text start)** entry, which should keep every stage in text.

While writing a disposable scene, switch to tty2 and back with Ctrl-Alt-F2/F1.
The other VT should appear promptly and the writing page should redraw on return.
Lock and unlock twice, then lock and sleep, wake and unlock. Pass when the same
scene returns, lock keystrokes never enter the book, and the display stays usable.
Check the warm-screen control before and after these handoffs. Record cold-start
time, unlock-to-login time, memory and battery use on the Celeron; VM results do
not establish those hardware measurements.

For the compositor comparison, use [GRAPHICAL_TRIAL](GRAPHICAL_TRIAL.md) and
`tools/graphical-trial.sh` with a disposable demo book. Record the HP Stream's
startup, memory, typing delay and power use for foot and OpenGL, then repeat
Colemak, compose, two locks and suspend/wake in each chosen session. Finish with
a day of writing. Native input and compositor warmth are still being built;
the current chroot/VM measurements are recorded separately in that document.

## Collecting the log

Kernel and system messages need administrator rights. From the PC, over SSH,
enter your password when asked:

```sh
ssh -t aegeorge@<laptop> 'sudo journalctl -b -k --no-pager | grep -E "rtw|lps|PM: suspend|PM: resume|ACPI: battery" ; sudo journalctl -b --no-pager -u "wros*" | tail -100'
```

Use `-b -1` instead of `-b` for the boot before the last restart. Send the
output with the results; it contains no book text.

## Results

| Release | Date | Check | Result | Notes |
| --- | --- | --- | --- | --- |
| 0.1.0~rc7 | 2026-11-06 | 4. School network | Pass | Joined the open, MAC-filtered school network from the Wi-Fi page. |
