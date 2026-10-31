# Coordination: OS infrastructure and the writing application

This file records the interfaces the OS/installer side and the writing application
(Grimoire editor, `grimoire-core`, `wros-shell`, `src/app`) share. Change a contract
here, or in the PR description, before changing either side. It describes the tree
at the time of writing; it is not a qualification claim.

## 1. Session start contract (stable; keep backward compatible)

1. `greetd` runs `/usr/bin/wros-greeter` as the locked, non-root `_greetd` account on
   tty1. There is no autologin and `source_profile = false` (`os/config/greetd.toml`).
2. After PAM succeeds, greetd starts the fixed command `/usr/libexec/writingdeck-session`
   (`wros-greeter` `SESSION` constant) as the authenticated writer, never root.
3. The wrapper sets `LANG=C.UTF-8`, `TERM` (default `linux`), `WRITINGDECK_OS=1`, and
   `WROS_LIVE_SESSION=1` when a live-session marker exists, applies `umask 077`, logs a
   journal marker, then `exec /usr/bin/grimoire` with **no arguments**.
4. The only environment flags `grimoire` reads from this layer are `WRITINGDECK_OS` and
   `WROS_LIVE_SESSION` (presence tests). Exiting `grimoire` returns to the login screen.

Do not add arguments or required environment to the `grimoire` spawn without updating
this section; the greeter and installer rely on the argument-free launch.

## 2. System adapter (`writingdeck-sys`): implemented vs refused

`Request`/`Reply` are the shared boundary. The Linux adapter fails closed (no process is
started) for everything marked refused. The simulator is only a test double.

| Area | State in the Linux adapter |
|---|---|
| Status, brightness, power-save, timezone, keyboard preview/confirm/cancel, display warmth | implemented, privileged parts through fixed polkit helpers |
| Wi-Fi radio on/off, scan, list saved profiles | implemented |
| Wi-Fi connect / disconnect / forget | **refused**: needs a protected secret transport (see §3) |
| Shutdown / restart | implemented; the UI saves first and refuses with dirty buffers or conflicts (#34) |
| Suspend | refused until a qualified lock-before-suspend exists (#32, #4) |
| Google status / connect / disconnect / Drive list / materialize | refused or "not configured" (see §4) |
| Backups (list/create/restore/verify) | **refused** (#10) |
| Update check | read-only `apt-get -s dist-upgrade`; stage/apply **refused** (#5) |
| Recovery status / reboot to recovery or firmware | **refused** (#5) |

## 3. Wi-Fi enrollment needs a secret transport (decision needed)

`Request::ConnectWifi { ssid }` deliberately carries no secret. A passphrase must never
travel in a `Request` (requests are serialized and may be logged), in `argv` (visible in
`/proc`) or in the environment. Proposed contract, for agreement before either side
changes: the UI hands the passphrase over a one-shot, non-logged channel (a `Zeroizing`
buffer on a pipe to the adapter) and the adapter feeds it to `nmcli --ask` on stdin;
the adapter returns only success/failure text. Adding a request variant is source
compatible only if no consumer matches `Request` exhaustively; check `src/app` first.

## 4. Google support: what is possible and what needs an owner decision

Findings (Google documentation read on 2026-10-31):

- The OAuth 2.0 **device flow** (the only flow that works on a keyboard-only TUI with a
  browser on another device) supports only `openid/email/profile`, `drive.file` and
  `drive.appdata` for Drive. It needs a registered "TVs and Limited Input devices" client
  and issues refresh tokens; Google treats such client credentials as non-confidential.
- `drive.file` and `drive.appdata` are non-sensitive scopes: **verified backup and
  conflict-safe sync (#10) are possible** against an app-created Drive folder or app data.
- **Browsing a writer's existing Drive (#29) needs `drive` or `drive.readonly`, which are
  restricted scopes** requiring Google OAuth verification (and a security assessment if
  data is stored on developer servers; WrOS is local-only). The device flow cannot grant them.
  Broad browsing therefore needs a different approved flow plus verification, or a product
  decision to ship folder-scoped access first.

Owner actions that code cannot replace: register a Google Cloud project and OAuth client,
configure the consent screen, and (for #29) start restricted-scope verification.

Infrastructure plan (OS side, no live account needed): a per-local-user encrypted
credential store outside manuscript and log paths, a device-flow state machine with an
injectable transport and mock tests (enrollment, cancel, expiry, slow-down, revocation,
offline, reboot, user isolation, log redaction), and an opt-in network gate so nothing
connects before enrollment. The editor-side sync/backup engine stays in `grimoire-core`.

**Status:** `crates/wros-google` implements the credential store (ChaCha20-Poly1305,
per-user key, AAD bound to the user name, 0700/0600, no symlinks), the device flow state
machine (scope allow-list, `interval`/`slow_down`, expiry, denial, cancel by drop, refresh
distinguishing revocation from offline, revoke) and an HTTPS-only transport. It is covered
by 16 unit tests with a mock transport. **Nothing links it yet**: `writingdeck-sys` still
refuses the Google requests. The wiring needs (a) the owner's OAuth client id/secret as
shipped configuration, and (b) agreement with the app on how the user code and
verification URL reach the TUI (proposal: a new `Reply` carrying `user_code`,
`verification_url`, `seconds_left`, polled by the app; no tokens ever cross the boundary).

## 5. Offline and telemetry rules the OS side enforces

The installer preseed disables NTP, apt mirror, service selection and the popularity
survey; the image disables NetworkManager's connectivity check; shipped crates make no
outbound requests, except `wros-google`'s HTTPS transport, which no binary calls yet. Any new network feature must be opt-in, user-enrolled and listed in
`docs/API_AND_NETWORK_USE.md`.
