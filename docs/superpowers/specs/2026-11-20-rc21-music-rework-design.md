# rc21: music rework (design)

Pending the owner's review (2026-11-20). The owner approved the chat summary plus media keys; the calls marked "(my call)" below are new. Laptop: HP Stream 11 on rc20.

## Why

rc20's Spotify support never played anything on the owner's laptop:
- Search and playlists needed a second, browser-based Spotify sign-in (spotify_player binds 127.0.0.1:8989). That is the "os error 98, failed to bind 127.0.0.1:8989" message.
- The phone's Connect handoff never produced a working speaker.
- The fallback we tried (spotifyd as an always-on Connect speaker) connects but skips every track with `error audio key 0 1`. librespot's maintainers say Spotify stopped giving decryption keys to unofficial players for accounts created after a cutoff, and that there is no workaround (librespot #1649, 2026-11-07). Every librespot-based player is affected: spotifyd, spotify_player and ncspot.

The owner also wants music out of the way of the writing desk:
- No separate music pane under the clearing.
- No Music on/off switch.
- No Music settings under "Look and keyboard".

## Goals

1. The clearing pane becomes a three-view carousel: Pomodoro → Visualizer → Music. ←/→ cycles the views, and clicking the switcher works as it does today.
2. The Music view shows what is playing and has the usual controls. It also offers rows to open the player (F7), change the source and sign in or out of the current source, so you never need Settings for these.
3. The separate music pane, its layout code and the Music on/off switch are removed. (my call) Music needs no switch: no player starts and nothing is polled until music is first used (the Music view shown, F4–F7, a media key, or a source chosen).
4. Settings gets a sixth group, **Music**, last in the list. The Music page moves into it from Look and keyboard.
5. Spotify is removed from WrOS:
   - spotify_player, librespot, the sign-in helper, its polkit action, the firewall set and the music-signin VM test all go.
   - On WrOS, Spotify stays in the source list as "not available yet" and can't be chosen.
   - A writer whose source was Spotify is moved to Music files.
   - (my call) spotify_player's folders `~/.cache/spotify-player` and `~/.config/spotify-player` (login, web token, logs) are deleted once at app start on WrOS, whatever the source, so no reusable credential stays on disk.
   - (my call) Off WrOS, Grimoire keeps its existing MPRIS/AppleScript remote control for the desktop Spotify app.
6. The laptop's hardware play/pause, next and previous keys work while writing, in any pane, and whether or not the Music view is showing. They go through triggerhappy and `wros-media-key`, like the brightness and volume keys (#80). The owner's HP Stream keyboard advertises KEY_PLAYPAUSE, KEY_NEXTSONG and KEY_PREVIOUSSONG (capability scan, 2026-11-20). Real presses are confirmed with `thd --dump` before Task 6 is built.

## Non-goals

- Spotify playback of any kind on WrOS. Spotify Soloist is planned for a later build: it needs a per-user API key, a user download and a refresh every 90 days.
- Renaming books to projects (#164 → rc22).
- New music sources, or changes to the F7 player overlay beyond removing Spotify.

## Anti-goals

- A music key that types into the book, or acts twice per press.
- Anything listening on the network for music.
- Any Spotify credential left on disk after the upgrade.
- A desk that starts a player or polls on its own before the writer asks for music.

## Approaches considered (Spotify)

1. **spotify_player with a login5 token from the handoff login.** Tested: the token gets HTTP 429 from the Web API on every call. Rejected.
2. **spotifyd as a Connect speaker.** Tested on the laptop: it connects, but every track fails with `audio key 0 1`. Rejected: Spotify blocks it.
3. **Spotify Soloist**, Spotify's official headless client. It works, but it can't be redistributed, it expires after 90 days and it needs a per-user API key. Deferred to a later build at the owner's choice.
4. **Remove Spotify for now.** Chosen.

## Decided details

- **Carousel order:** Pomodoro, Visualizer, Music. The switcher reads "◂ pomodoro visualizer music ▸".
- **Music view keys:**
  - space play/pause; `[` / `]` previous/next; `+` / `-` volume
  - ↑/↓ move between the rows; Enter acts on the selected row
  - ←/→ switch views, as in the other two views; seeking stays in the F7 player
- **Music view rows:**
  - "Open player · F7"
  - "Source · <label>…" (opens the existing source picker)
  - For Jellyfin and Plex only: "Sign in to <label>…", which opens Settings › Music › Set up server, or "Sign out of <label>", which forgets the server login.
- **Media keys:** (my call: they do nothing on the lock screen; presses made there are dropped at unlock) wros-media-key gains the actions `play-pause`, `next` and `previous`. It records each press in `/run/wros-keys/music`. The app watches that file and acts on each new press once, and ignores presses recorded before it started.

## Left to the builder

The exact layout of the Music view's 9 rows, wording beyond the labels above, and how the app's watcher thread is structured.
