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

**Pass when:** each close saves first (the sentence is there after unlock),
the screen after waking is the lock, never the book; the Colemak password works
at the lock; a wrong password is refused; after step 5 the laptop stayed awake
and locked. Nothing you typed at the lock shows up in the book.

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
2. Open a web check that needs the network (Settings › System › Updates ›
   Check for WrOS updates is enough).

**Pass when:** the network appears in the scan, joins, and the update check
reaches the internet. If the school filters by hardware address, joining at
all shows the laptop kept its registered address.

## 5. Low battery

1. Unplug the charger with a disposable book open and leave the laptop running
   (keep the lid open, screen on) until the battery is nearly empty.
2. WrOS saves and suspends on its own when the battery reports critical or
   drops to 8 % while discharging.
3. Plug in, wake it, unlock.

**Pass when:** it suspended before the battery died, the last words you typed
before it suspended are in the book, and it woke to the lock.

## 6. Shutdown and restart during a backup

1. Start a backup of the disposable book (Settings › Accounts › Backups), and
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
- Settings › System › Updates shows the new version, and offers to go back to
  the previous one.

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
| | | | | |
