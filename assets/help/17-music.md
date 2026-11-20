# Music

Open *Settings › Look and keyboard › Music*. Choose **Music · off** to turn music on, and **Source** to cycle through the available sources.

| Source | How it works |
|---|---|
| YouTube Music | Drives the th-ch desktop app through its API Server. Setup on the WrOS Music page is planned. |
| Spotify | Drives the Spotify desktop app on macOS and Linux. |
| Jellyfin | Choose **Set up Jellyfin server…** and sign in with your server URL, username and password. |
| Plex | Choose **Set up Plex server…** and enter your server URL and X-Plex-Token. The form shows where to find the token. |

WrOS never pulls audio out of YouTube Music or Spotify — it presses the buttons of the app you already pay for.

## From anywhere

- `F5` play / pause, `F4` previous, `F6` next.
- `F7` opens the player.

## The music pane

`Tab` to the music pane and it answers the player's keys:

- `[` and `]` previous and next track; `←` `→` skip ten seconds back and forward.
- `Space` plays and pauses.
- `r` repeat, `s` shuffle, `+` and `-` volume, `l` like.
- `Enter` opens the player.

The pane shows what's playing and, beside the time, `↻` for repeat, `↻1` for repeat one, `⇄` for shuffle, `♥` if you like the song, and the volume when it's below full. Each key says what it did in the status bar ("repeat: one", "volume 80").

## The player

`F7` (or `Enter` on the music pane) opens the player: what's playing with a progress bar, then three lists that `Tab` switches between.

- **Queue**: everything queued. `Enter` jumps to a song.
- **Playlists**: your YouTube Music playlists. `Enter` plays one; `a` queues it after the current song. `/` searches every public playlist; `Esc` brings yours back.
- **Search**: `/`, type, `Enter`. `Enter` on a result plays it now; `a` plays it next.

The same keys as the music pane work everywhere in the player, and it shows repeat, shuffle, volume and like under the progress bar. `Esc` or `F7` closes it.
