# Music

Music lives in the clearing under the outline, next to the Pomodoro and the Visualizer: `Tab` to the clearing and press `←` or `→` until it says **music**. Nothing plays or connects until you ask for music: showing the Music view, pressing a music key or choosing a source starts it.

The Music view shows what's playing and three rows, chosen with `↑` `↓` and `Enter`:

- **Open player · F7** opens the player (below).
- **Source · …** picks where music comes from.
- **Sign in to …** or **Sign out of …**, for Jellyfin and Plex.

The same choices are in *Settings › Music*, and *Music source* in the menu picks a source from a list.

| Source | How it works |
|---|---|
| Music files | Plays the music in `~/Music` and on a plugged-in USB stick. Nothing to set up. In the player, **Playlists** lists your album folders and **Search** finds songs by name. |
| YouTube Music | Search songs, albums and playlists in the player, with no sign-in. While you type a search, `Tab` switches between YouTube Music and all of YouTube (videos, sound only); WrOS remembers which. A song starts playing in about 10 seconds, while it arrives. *Settings › Music* can make songs download to a small cache first instead. Your own library and liked songs come in a later update. |
| Jellyfin | Choose **Sign in to Jellyfin…** and sign in with your server address, username and password. |
| Plex | Choose **Sign in to Plex…** and enter your server address and X-Plex-Token. The form shows where to find the token. |
| Spotify | Not available on WrOS yet. Spotify no longer lets players like WrOS's unlock its music for newer accounts. Spotify's own player is planned for a later update. Elsewhere, Grimoire controls the Spotify desktop app while it runs. |

The YouTube Music player isn't an official app. A change on YouTube's side can stop it working until a WrOS update fixes it.

## From anywhere

- `F5` play / pause, `F4` previous, `F6` next.
- `F7` opens the player.
- The laptop's own play/pause, next and previous keys do the same while you write. On the lock screen they do nothing (volume still works there).

## The Music view

With the clearing on music:

- `[` and `]` previous and next track.
- `Space` plays and pauses.
- `r` repeat, `s` shuffle, `+` and `-` volume.
- `↑` `↓` choose a row, `Enter` does it.
- `←` `→` go back to the Pomodoro or the Visualizer.

Beside the time it shows `↻` for repeat, `↻1` for repeat one, `⇄` for shuffle, `♥` if you like the song, and the volume when it's below full. Each key says what it did in the status bar ("repeat: one", "volume 80").

## The player

`F7` (or **Open player** in the Music view) opens the player: what's playing with a progress bar, then three lists that `Tab` switches between.

- **Queue**: everything queued. `Enter` jumps to a song.
- **Playlists**: your album folders (Music files) or your playlists. `Enter` plays one; `a` queues it after the current song. `/` searches albums and playlists; `Esc` brings yours back.
- **Search**: `/`, type, `Enter`. `Enter` on a result plays it now; `a` plays it next.

In the player, `←` `→` skip ten seconds back and forward and `l` likes the song; the Music view's other keys work there too. It shows repeat, shuffle, volume and like under the progress bar. `Esc` or `F7` closes it.
