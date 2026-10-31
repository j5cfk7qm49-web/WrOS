# APIs and network use

This is an audit of the current Debian application source, separate from build
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
PDF conversion uses the existing headless LibreOffice adapter. Export preserves
URL text without fetching the linked resource. These features do not call a
hosted AI, online grammar or cloud document service.

Read-aloud reuses `src/voice.rs`: installed Piper and local voice models,
Speech Dispatcher through `spd-say`, or eSpeak NG. These are local adapters to
available backends, not two WrOS speech engines or a cloud speech API.

## Local writing history

Session history uses the maintained Git CLI. The inherited app automatically
pushed when a configured remote existed; an actual generated-book/local-remote
probe reproduced that transfer at startup. The application now removes that
background worker and its startup/save calls. Opening, saving, quitting and
reopening a book preserve local session snapshots without starting a Git push.
Adding a remote is not treated as consent to send manuscript history.

The upstream core library's explicit `sessions::push` helper is retained for
compatibility/tests; WrOS has no application call site for it. The sessions
screen does not label local commits or cached tracking refs as a verified backup.
Native encrypted backup/restore is now explicit through the workflow below. Separately configured sync
clients and external Git tools operate outside this application workflow.

## Optional music

Music is disabled by default in `music::Config`. Enabling a source permits its
music requests; it does not send manuscript text.

| Integration | Actual interface and data |
|---|---|
| YouTube Music companion | Configured HTTP host, default `127.0.0.1:26538`; `/auth/grimoiretui` pairing and `/api/v1/` player state, search, queue and playback controls. Pairing uses a local bearer token. |
| YouTube playlist browsing | POST `https://music.youtube.com/youtubei/v1/browse?prettyPrint=false` with playlist/browse identifiers and web-client context. This is an inherited website endpoint, not the official YouTube Data API. |
| YouTube Music setup | GitHub releases API at `https://api.github.com/repos/th-ch/youtube-music/releases/latest`, then the selected release asset. This is companion setup, not WrOS signed system updates. |
| Spotify | Installed player's local MPRIS interface via `playerctl` on Debian; WrOS does not use Spotify's OAuth/Web API. The Spotify player handles its own service connection. |
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
Restic's rclone backend and can contact Google Drive/Google token endpoints.
No background startup, autosave, history or ordinary writing path starts these
provider calls. Failed/offline backup jobs record the project in the local private queue (`queue.json`) and require an explicit retry (`run-queue` or `Create encrypted backup`). `check-google` and `disconnect-google` manage local Google Drive credentials explicitly.

Authorization JSON travels only through the masked field and private stdin
protocol, never command arguments, UI notices, manuscript files or diagnostic
logs. Dedicated rclone configuration/recovery keys are private per-user files
outside projects. Restic encrypts staged project content and metadata before
upload. Rclone owns OAuth token refresh. The entire project folder (including
private notes and project metadata) is included by explicit backup; export's
private-note exclusion policy does not apply to disaster-recovery backups.
No real account was enrolled or manuscript transmitted during qualification.
See [scope and recovery](NATIVE_BACKUP.md).

## Debian integration and unavailable services

The native backend invokes local `nmcli` (NetworkManager), `timedatectl`
(systemd time service), `findmnt`, cached `apt-get -s dist-upgrade`,
`brightnessctl` and explicit supported `systemctl` operations. It reads local
account/status information. A cached APT simulation is not a repository refresh
or installed update. Wi-Fi scanning is radio activity, not an internet API. Native Wi-Fi connection and saved-profile management invoke `nmcli` explicitly on user confirmation, passing WPA passphrases only via private `stdin` to `nmcli --ask` and never in command arguments, `Debug` output, or serialized requests.

General Drive browsing/materialization, cloud sync, update application and
recovery/firmware operations remain unavailable and fail before their tools run.
The separate backup adapter provides explicit authorization/backup/restore;
the old general-purpose system request variants remain unavailable. No successful OAuth enrollment,
Drive API call or manuscript upload is fabricated. No telemetry or mandatory
cloud service is introduced by these writing integrations.

## Verification

Focused strict Clippy and compiled four-size local-session/privacy/Undo/restart
workflows passed, including a remote added while the editor was running. The
original startup-transfer failure is retained privately. The [independent full Debian run](evidence/local-session-privacy-local.json) passed
1077 Rust tests per configuration, 187 Python tests with zero skips, both Clippys and 21 gates. [Provenance and frames](evidence/local-session-privacy-local-provenance.json) bind its source and tested binary.
