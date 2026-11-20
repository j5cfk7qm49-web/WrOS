# Music

Music is **off until you turn it on**: open *Settings › Look and keyboard › Music* and choose **Music · off** (*Turn music on* in the menu does the same). **Source** moves to the next source, and *Music source* in the menu picks one from a list. Each source shows its own setup rows under it.

| Source | How it works |
|---|---|
| Music files | Plays the music in `~/Music` and on a plugged-in USB stick. Nothing to set up. In the player, **Playlists** lists your album folders and **Search** finds songs by name. |
| YouTube Music | Search songs, albums and playlists in the player, with no sign-in. Each song downloads to a small cache before it plays: the first takes 10–20 seconds, and the next one downloads while one plays. Your own library and liked songs come in a later update. |
| Spotify | Needs Spotify Premium. Choose **Sign in to Spotify…**, then on your phone, on the same Wi-Fi, open Spotify, tap the speaker icon and choose **WrOS**. For those three minutes this laptop accepts the phone's connection; afterwards nothing listens. Search, your playlists and playing then work from the player, and your phone can control it too. **Sign out of Spotify** removes the sign-in. |
| Jellyfin | Choose **Set up Jellyfin server…** and sign in with your server address, username and password. |
| Plex | Choose **Set up Plex server…** and enter your server address and X-Plex-Token. The form shows where to find the token. |

The YouTube Music and Spotify players aren't official apps. A change on their side can stop them working until a WrOS update fixes it.

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
- **Playlists**: your album folders (Music files) or your playlists (Spotify). `Enter` plays one; `a` queues it after the current song. `/` searches albums and playlists; `Esc` brings yours back.
- **Search**: `/`, type, `Enter`. `Enter` on a result plays it now; `a` plays it next.

The same keys as the music pane work everywhere in the player, and it shows repeat, shuffle, volume and like under the progress bar. `Esc` or `F7` closes it.
