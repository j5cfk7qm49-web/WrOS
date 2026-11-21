# APIs and network use

This is an audit of the current Debian and NixOS application source, separate from build
downloads and optional services installed or configured outside WrOS. It is not
a claim that the entire operating system is finished.

## Writing and revision

Editor saves, history, annotations, continuity, milestones, variants, comparisons
and restore operate on local files and Rust models. The shared document registry
uses `ReviewNotes::register` and guarded `Store::update`/`verify`; it does not use
a web API. Markdown/YAML remains canonical. The source identity guard refuses
missing or unsupported linked metadata before assigning replacement identities.

Writing checks use local Spellbook/optional Harper, Proselint and Nabokov/spaCy.
Dictionary and thesaurus lookups use local WordNet. Word/EPUB generation is local;
PDF conversion uses the existing headless LibreOffice adapter when installed. Export preserves
URL text without fetching the linked resource. These features do not call a
hosted AI, online grammar or cloud document service.

Read-aloud uses local adapters in `src/voice.rs`. The NixOS image supplies
eSpeak NG. Piper requires a separately available executable and local voice
model; Speech Dispatcher is another optional backend, not an enabled service in
the standard image. These adapters do not use a cloud speech API.

## Writing history

On WrOS, the Git CLI records a local timeline for every book, including small
saves, marked writing sessions and changes brought in from Drive. Scene history
reads the local repository. A book with readable Drive sync enabled also sends
and fetches Git bundles through `wros-sync`. Outside WrOS, Writing sessions
remains opt-in and local. A configured Git remote never starts a Git push.

The upstream core library's explicit `sessions::push` helper is retained for
compatibility/tests; WrOS has no application call site for it. The sessions
screen does not label local commits or cached tracking refs as a verified backup.
Native encrypted backup/restore is now explicit through the workflow below. Separately configured sync
clients and external Git tools operate outside this application workflow.

## Optional music

Music makes no requests until it is first used: the Music view shown, a music key,
or a source chosen. A source then makes its own music requests; none sends
manuscript text.

| Integration | Actual interface and data |
|---|---|
| YouTube Music companion | Configured HTTP host, default `127.0.0.1:26538`; `/auth/grimoiretui` pairing and `/api/v1/` player state, search, queue and playback controls. Pairing uses a local bearer token. |
| YouTube playlist browsing | POST `https://music.youtube.com/youtubei/v1/browse?prettyPrint=false` with playlist/browse identifiers and web-client context. This is an inherited website endpoint, not the official YouTube Data API. |
| YouTube Music setup | GitHub releases API at `https://api.github.com/repos/th-ch/youtube-music/releases/latest`, then the selected release asset. This is companion setup, not WrOS signed system updates. |
| Spotify | Off WrOS only: the Spotify desktop app's local MPRIS interface via `playerctl` (AppleScript on macOS). WrOS ships no Spotify player and makes no Spotify requests; Spotify can't be chosen there yet. |
| Jellyfin | Configured server HTTP API: `/Users/AuthenticateByName`, `/Items` and `/Audio/{id}/stream`. Login credentials/token go to that selected server. |
| Plex | Configured server HTTP API: `/library/sections`, `/library/sections/{id}/all` and returned media stream paths, using the configured Plex token. |

Relevant implementation is in `src/music/{mod,setup,ytm,spotify}.rs` and
`src/library.rs`. Source host settings can select another machine; loopback is
the YouTube companion default, not a guarantee for every configuration.

## Explicit encrypted backup and Google Drive

`src/app/backup.rs`, `src/backup_service.rs` and the unprivileged
`os/scripts/wros-backup` adapter invoke maintained Restic for encrypted repositories
and rclone for the optional Drive backend. Opening Settings reads local state
only. Local repository backup/restore uses local filesystem IO. Google connection
explicitly validates authorization via `rclone about`; Drive repository creation,
snapshot refresh, backup, restore and stale-lock recovery explicitly invoke
Restic's rclone backend and can contact Google Drive/Google token endpoints
(`oauth2.googleapis.com`, `www.googleapis.com`).
After backup is set up for a book, saved changes
can trigger an automatic encrypted backup after ten minutes without editing,
normally at most hourly. Failed automatic runs retry after 15 minutes, an hour,
then every four hours. A Drive repository contacts Google during those runs. Manual
failed jobs remain in the private queue (`queue.json`) for explicit retry (`run-queue` or
`Create encrypted backup`).

Google Drive can be linked by phone, masked authorization input or a private
authorization file from a USB stick. Helper requests use private stdin;
authorization secrets are not passed in command arguments or written to
manuscript files. Dedicated rclone configuration and recovery keys are private
per-user files outside projects. Restic encrypts staged project content and
metadata before upload. Rclone owns OAuth token refresh. The entire project folder
(including private notes and project metadata) is included in each backup; export's
private-note exclusion policy does not apply to disaster-recovery backups.
No real account was enrolled or manuscript transmitted during qualification.
See [scope and recovery](NATIVE_BACKUP.md).

## Readable Google Drive sync

`src/sync_service.rs` runs the unprivileged `wros-sync` helper at its compiled
Nix store path, which uses the backup's rclone configuration (`lsjson --hash`,
`copyto`, `moveto`, `cat`, `rcat`) against the `WrOS/` folder
of the connected Drive. Nothing
is contacted until the writer turns sync on for a book; then the app syncs that book
when it opens, 90 seconds after the last save or edit and every 5 minutes while it
is open. The app also requests a final sync after saving before closing a book
or shutting down; retry the action once the run finishes. Sleep starts a final sync
without delaying the lock. It sends only
files that changed since the last agreed version. Those copies are plaintext.
Offline runs fail quietly and wait for the next one.

For a synced book, the helper also lists, verifies, uploads and fetches Git
bundles under `WrOS-History/<book>/timeline/` using the same rclone credentials.
Uploads run at session end, hourly during a long session and after reconnect.
They carry the journal, productivity figures and review notes in plaintext, like
the book. A full bundle can be read by any Git client using
`git clone "<file>.bundle"`. Normal sync no longer makes dated
`WrOS-History/<book>/<date>/` copies. It still makes a dated copy before replacing
or removing a local file if the timeline checkpoint fails.

## Local system integration

The NixOS backend invokes local `nmcli`, `timedatectl`, `findmnt`,
`brightnessctl`, WrOS helpers and supported `systemctl` operations. The retained
APT simulation request is unavailable on NixOS. Wi-Fi scanning is radio activity,
not an internet API. Confirmed Wi-Fi connections pass WPA passphrases through
private stdin to `nmcli --ask`, never command arguments.

New NixOS installs enable `systemd-timesyncd` with `0.nixos.pool.ntp.org` through
`3.nixos.pool.ntp.org`. While enabled and online, it sends NTP requests. Settings
can set the clock manually and return to network time. The NixOS installer does
not configure Debian APT sources or apt-daily timers; system updates use the
signed WrOS NixOS channel.

An installed NixOS WrOS checks for updates after login and then daily, with
earlier retries after failed checks. The writer can also check manually. The
check reads signed `InRelease` metadata and `closure-manifest.json` from
`https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/`. Choosing **Update WrOS**
downloads the required binary-cache files and verifies their signed sizes and
hashes before staging. The separate `updates/` APT channel remains for Debian
rc13 systems.

Drive browsing and copying use the backup adapter. Signed update application,
go-back and repair use the WrOS update helpers. The retained general-purpose
Drive/backup request variants and `RebootRecovery`/`RebootFirmware` requests remain
unavailable; they do not describe the availability of the separate native
workflows. No successful OAuth enrollment, Drive API call or manuscript upload
is fabricated. No telemetry or mandatory
cloud service is introduced by these writing integrations.

## NixOS (`0.1.0~rc14+`) network hosts and online update channel (`D2`)

Built-in network services and optional integrations are listed below. DHCP, DNS
and user-configured server addresses depend on the network and configuration.

| Service / Feature | Host(s) contacted | Protocol | When contacted |
|---|---|---|---|
| Network time (`systemd-timesyncd`) | `0.nixos.pool.ntp.org`, `1.nixos.pool.ntp.org`, `2.nixos.pool.ntp.org`, `3.nixos.pool.ntp.org` | UDP/123 (NTP) | Automatically while online when network time is enabled (`services.timesyncd.enable = lib.mkDefault true` in `nixos/modules/network.nix`) |
| Online update check & binary-cache metadata (`wros-update check-online`, `wros-update-fetch`) | `j5cfk7qm49-web.github.io` (`https://j5cfk7qm49-web.github.io/WrOS/nixos-updates/`) | HTTPS (TCP/443) | After login, daily, on retries after failed checks or on manual checks for `InRelease` + `closure-manifest.json`; fetches required `cache/nix-cache-info`, `cache/<hash>.narinfo` and `cache/nar/*.nar.zst` files only when the writer explicitly runs **Update WrOS** |
| Optional Google Drive backup / sync (`wros-backup`, `wros-sync` via `rclone`) | `oauth2.googleapis.com`, `www.googleapis.com` | HTTPS (TCP/443) | When linking or validating an account, revoking a supported grant, browsing or copying Drive files, refreshing or operating on a Drive backup repository, running configured automatic backups, or syncing an enabled book |
| Optional music integrations (idle until first used) | `music.youtube.com`; the configured YouTube companion host, defaulting to loopback; `api.github.com` and the selected companion release-asset URL during setup; user-configured Jellyfin/Plex hosts | HTTP/HTTPS | Explicit companion setup contacts the releases API, selected asset URL and pairing host; an enabled music source makes requests while it runs |

The current publisher serves cache files through Pages and rejects oversized
files. The updater refuses redirects; GitHub release-asset redirects are not a
working update download path.

NetworkManager connectivity checking is disabled (`[connectivity] enabled=false` in `/etc/NetworkManager/conf.d/60-wros-no-connectivity-check.conf`), and Nix substituters and channels are disabled (`nix.channel.enable = false`, `nix.settings.substituters = lib.mkForce []`), so built-in Nix operations do not use `cache.nixos.org` as a substituter. The Debian channel `https://j5cfk7qm49-web.github.io/WrOS/updates/` remains untouched for `v0.1.0-rc13` systems.

## Verification

Focused strict Clippy and compiled four-size local-session/privacy/Undo/restart
workflows passed, including a remote added while the editor was running. The
original startup-transfer failure is retained privately. The [independent full Debian run](evidence/local-session-privacy-local.json) passed
1077 Rust tests per configuration, 187 Python tests with zero skips, both Clippys and 21 gates. [Provenance and frames](evidence/local-session-privacy-local-provenance.json) bind its source and tested binary.
