//! Albums and tracks from ~/Music and mounted USB sticks.

mod library;
mod playback;

pub(crate) use library::file_counts;
pub(super) use playback::Files;

/// Real mpv, real files: run with `--ignored` in a private HOME whose mpv.conf says `ao=null`
/// (rc20-work/live-music.sh). Not part of the normal suites: it needs mpv and plays audio.
#[cfg(test)]
#[cfg(unix)]
mod live {
    use super::Files;
    use crate::music::{Backend, Cmd, State};
    use std::{
        fs,
        path::Path,
        time::{Duration, Instant},
    };

    /// A short mono 8 kHz WAV with a tone, `secs` long.
    fn wav(path: &Path, secs: f32) {
        let rate = 8000u32;
        let samples: Vec<i16> = (0..(rate as f32 * secs) as u32)
            .map(|i| {
                ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16
            })
            .collect();
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data.len() as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * 2).to_le_bytes());
        out.extend(2u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        fs::write(path, out).unwrap();
    }

    fn playing(files: &mut Files, want: &str, within: Duration) -> f64 {
        let end = Instant::now() + within;
        loop {
            if let Ok(State::Playing(t)) = files.state()
                && t.title.contains(want)
                && t.progress > 0.0
            {
                return t.progress;
            }
            assert!(
                Instant::now() < end,
                "{want} never played: {:?}",
                files.state()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    #[test]
    #[ignore = "needs mpv; rc20-work/live-music.sh"]
    fn a_folder_plays_moves_on_and_answers_the_keys() {
        let album = grimoire_core::paths::home().join("Music/Live Album");
        fs::create_dir_all(&album).unwrap();
        wav(&album.join("01 First.wav"), 2.0);
        wav(&album.join("02 Second.wav"), 8.0);
        let mut files = Files::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        files
            .load_playlist(
                album.to_string_lossy().into(),
                "Live Album".into(),
                true,
                tx,
            )
            .unwrap();
        playing(&mut files, "First", Duration::from_secs(10));
        // The first ends by itself and the second follows.
        playing(&mut files, "Second", Duration::from_secs(10));
        let queue = files.queue().unwrap();
        assert_eq!(queue.len(), 2);
        assert!(queue[1].current, "{queue:?}");
        files.command(Cmd::Prev).unwrap();
        playing(&mut files, "First", Duration::from_secs(5));
        files.command(Cmd::Next).unwrap();
        playing(&mut files, "Second", Duration::from_secs(5));
        files.command(Cmd::PlayPause).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        match files.state().unwrap() {
            State::Playing(t) => assert!(!t.playing, "paused"),
            other => panic!("{other:?}"),
        }
    }
}
