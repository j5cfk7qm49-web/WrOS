# rc5: phone Drive linking, automatic backups, undoing a bad update, recovery boot

## Why

WrOS is about to ship releases quickly. Every release must be safe to try:
a book is never more than a short while away from a verified backup, a bad
update can always be undone, and a computer that will not start the writing
session can still be brought back. Today backups run only when started by
hand, a completed update cannot be undone (only repaired), and recovery
needs the install media.

Backups and sync to Google Drive also need Drive connected, and today that
takes `rclone authorize drive` on a second computer. rclone's shared Google
sign-in is also being retired during 2026, after which today's connections
stop working.

## Goals

1. **Link Google Drive from the writer's phone**, with no second computer,
   with full Drive access, so files added to the book away from WrOS
   appear in WrOS.
2. **Automatic backups** of the open book, verified like manual ones.
3. **Go back to the previous version** of WrOS, by hand or automatically
   when a new version cannot start the writing session.
4. **A recovery boot**: a boot-menu entry that starts a small safe mode able
   to go back a version, repair packages and restore a book, even when the
   normal session will not start.
5. **A system profile in each backup**, so a fresh install plus the recovery
   key brings back settings as well as books.

## Non-goals

- Whole-disk snapshots. The root filesystem is ext4 on LUKS; snapshots would
  need a different disk layout. That belongs with the installer work.
- Backing up Debian itself. WrOS updates change only the `wros` package, so
  the previous `wros` package is what a rollback needs.
- Secure Boot (left for the installer work).
- Google's app verification. WrOS stays under the personal-use exemption
  (fewer than 100 users); sign-in shows an "unverified app" notice.
- Signing in to WrOS itself with Google. The WrOS login stays the local
  password, which works offline.

## Anti-goals

- A backup that runs while words are unsaved, or reports success without a
  verified restore (the existing gate stays: save, snapshot, restore, compare).
- A rollback that installs anything not previously verified and installed on
  this computer.
- A recovery mode that opens the computer to whoever holds it.
- A linking flow that hands Drive access to anything but the laptop that
  showed the code, or that needs a WrOS server.

## Design

### 1. Linking Google Drive from the phone

Proven end to end on the writer's laptop and iPhone on 2026-11-04: full
`drive` access, a refresh token that renews, and rclone reading Drive with it.

**The writer's steps.** Settings › Backups › **Link Google Drive**:

1. The laptop shows a QR code.
2. The writer scans it with their phone and signs in to Google.
3. They click through the unverified-app notice and tap Allow.
4. On the WrOS page that opens, they tap **Finish linking**.
5. The laptop says "Google Drive linked".

**How it works:**
- **WrOS's own Google client.** Google Cloud project `wros-513717` (setup in
  `docs/GOOGLE_SIGN_IN_SETUP.md`) owns a Web application client
  ("WrOS linking") with the redirect
  `https://j5cfk7qm49-web.github.io/WrOS/link/`.
- **The secret stays out of the public repository.** Google does not treat
  a client secret as secret for this kind of client, but it is still kept
  out of the repo, in a local file the release build puts into the package
  (`/usr/share/wros/google-client.json`, root-owned, readable by the backup
  helper). A build without that file hides Link Google Drive and says why.
  The client ID is public: it appears in every sign-in address.
- **The QR link.** The laptop makes a PKCE pair. The QR code holds only
  `https://j5cfk7qm49-web.github.io/WrOS/l/#<S256 challenge>.<address>`,
  where `<address>` is the laptop's private IPv4 address and a listening
  port, packed into 6 bytes as base64url. That is about 96 characters, QR
  version 6, drawn as two full blocks per module: 90×45 cells, which fits
  the 170×48 console. The console font has no half blocks.
- **The site's two static pages:**
  - **`/l/`** sends the phone to Google's consent screen. It asks for the
    `drive` scope with `access_type=offline` and `prompt=consent`, passes
    the challenge, and puts the address in `state`.
  - **`/link/`** receives the code. It checks that the address is private
    (10/8, 172.16/12 or 192.168/16, port 1024 or above), clears the code
    from the address bar, and shows **Finish linking**, which goes to
    `http://<address>/linked?code=…`.
  - Safari allows that jump only after a tap, so the button stays.
  - Neither page stores or sends anything else.
- **The laptop end.** A short-lived listener runs inside the backup helper,
  not the app. It opens only while the QR code is on screen and closes
  after 10 minutes or after one successful link. It takes the first code,
  swaps it (with the verifier and the client secret) for a token, and
  writes the `wros-google` remote with `client_id`, `client_secret`,
  `scope = drive` and the token. It then runs the existing **Verify Google
  Drive connection** check, and answers the phone "Linked" or with the
  reason.
  - A code sent by anyone else on the network fails, because only this
    laptop holds the verifier.
  - The verifier lives only in the helper's memory.
- **When the phone cannot reach the laptop.** This happens with guest or
  school Wi-Fi that keeps devices apart, or when the two are on different
  networks. A page cannot tell when the browser fails to open another
  address, so `/link/` says under the button what to do if the laptop does
  not answer: turn on the phone's hotspot, connect the laptop to it, and
  scan the new code the laptop then shows (its address has changed). As a
  last resort, a "Show the code" link displays the
  code, and the laptop's QR screen accepts it typed in. *My call.*
- **Existing connections** made with `rclone authorize` keep working until
  rclone's shared sign-in is retired. Settings shows "Relink with your
  phone" for them; linking replaces the old remote.
- **Disconnect** also revokes the token at Google
  (`https://oauth2.googleapis.com/revoke`), and then removes it as today.
- The Drive folder picker can drop its folder-scoped `drive.file` limit and
  browse the writer's whole Drive.
- **Not used:** Google's device flow ("enter this code at google.com/device").
  It cannot grant `drive`, and it refuses `drive.appdata` too, so it cannot
  give WrOS access to files added away from WrOS.
- **Site deployment:**
  - The static pages live in `pages/` in the repository. The release
    workflow copies them into every Pages deploy beside `updates/`, because
    each deploy replaces the whole site.
  - The privacy policy (`pages/privacy/`) states that the one-use code
    passes through the site's address on its way to the laptop, and is
    useless without the laptop's half of the PKCE pair.
  - Today the pages are deployed only from the spike tag
    `v0.1.0-rc4-linktest3`. They must land in development before any other
    deploy, or that deploy erases them.

### 2. Automatic backups

- Applies to a book once backups are set up for it (a repository exists);
  nothing new to switch on.
- **When:** after saved changes that are not yet in a verified backup, once the
  writer has paused for 10 minutes, and at most once an hour; also as the first
  step of every update (the existing back-up-then-stage chain). *My call.*
- **Failures** retry by themselves: offline or provider errors back off
  (15 min, 1 h, 4 h) and run again when the update check next finds the network.
  The queue that exists today becomes this retry list.
- **Quiet:** a status line in Backups and on the home screen ("Backed up
  14:02 · verified"); a notice only when backups have failed for a day.
- **Retention**, so automatic backups do not grow the repository without end
  (restic `forget --prune`, after a successful backup): every snapshot from the
  last 7 days, one a day for 90 days, one a week after that. Manual snapshots
  and named ones are never pruned. *Agreed with the writer (2026-11-04).*
  Space: the repository lives on Drive or a backup drive, not the laptop;
  restic stores only changed chunks, so a novel-sized book with hundreds of
  snapshots stays in the tens of MB. The only local cost is restic's cache,
  which is bounded and safe to clear.

### 3. System profile

Each backup also stores a small `wros-profile/` folder beside the book (inside
the same encrypted snapshot): the writer's Grimoire settings, WrOS interface
preferences, keyboard layout and timezone, and the list of books with their
backup and sync settings. **Not included:** the recovery key (it unlocks the
backup), saved Wi-Fi passwords, and the Google connection token: after a
fresh install, linking again from the phone takes about a minute (section 1). Restore offers
"Restore settings too" when a profile is present.

### 4. Going back to the previous version

- After every successful install, the engine keeps the package it replaced (the
  previous `wros` .deb, its signed index and `InRelease`) in
  `/var/lib/wros-update/previous/`, re-verified against the pinned key when it is
  used. Only one previous version is kept.
- **Go back** (Settings › Updates and the recovery boot) stages that package as
  an explicit rollback: the engine's anti-replay check (newer release dates only)
  allows exactly the retained previous version and nothing else. It installs at
  the next restart, or now if it is app-only.
- **Automatic:** after an update installs at boot, WrOS marks the boot "on
  probation". When the writing session draws its first screen it reports
  healthy and the mark clears. If two boots in a row end without that report,
  the next boot goes back to the previous version before the login screen and
  says so after login.
- The first rollback is possible from the update after rc5: rc5's own install
  is done by rc4's engine, which keeps nothing.

### 5. Recovery boot

- A "WrOS recovery" boot-menu entry that boots the same kernel into
  `wros-recovery.target`: no greeter, no network unless asked, a console menu.
- **Protected:** the menu asks for the WrOS account password (the disk is
  already unlocked by its LUKS passphrase at boot). *Agreed with the writer.*
- **Choices:** go back to the previous version; repair packages (today's
  Repair); restore a book from a local or Drive backup into a new folder (asks
  for the recovery key); show what happened (the update journal); continue to
  the normal login; power off.
- The boot menu shows the entry after a failed boot, and stays hidden
  otherwise. Debian keeps the previous kernel, so that menu also offers it.

## What is tested where

- Python unit tests for linking:
  - building the QR link and packing the address;
  - the listener, which takes one code and refuses a second;
  - the token exchange against a fake Google endpoint;
  - the remote written with WrOS's client;
  - revoke on disconnect;
  - the build without a client file.
- A page test, run in Node or a headless browser, for `/l/` and `/link/`.
  It checks the private-address check, the error answers from Google, and
  that the code is cleared from the address bar.
- `os/tests/test_update_workflow.py` covers the static pages step.
- The real phone, last: iPhone Safari (proven in the spike) and an Android
  phone in Chrome, which may ask for permission to reach devices on the
  network.
- Python unit tests for the engine (retention of the previous package, the
  explicit rollback exception, probation and the automatic rollback), the
  backup scheduler and retention, and the profile contents.
- Rust UI tests for Settings, Backups status and the home-screen line.
- The rc-vm harness for the real thing: install rc5, apply a deliberately broken
  update in a VM, and check that it rolls back by itself; boot the recovery
  entry and use each choice.
- The laptop last, after the VM.

## Left to the builder

Exact timings and wording, the listener's port choice, the profile's file list, how probation is recorded,
and the recovery menu's layout.
