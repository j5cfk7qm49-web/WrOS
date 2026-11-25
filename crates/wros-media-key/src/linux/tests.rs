use std::collections::BTreeMap;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::mpsc;

use super::volume::{PlaybackMixer, change_volume};
use super::*;

struct Invocation {
    program: OsString,
    args: Vec<OsString>,
    env: BTreeMap<OsString, Option<OsString>>,
    timeout: Duration,
}

struct FakeMixer {
    range: (i64, i64),
    raw: i64,
    switch: Option<bool>,
    fail: bool,
}

impl Default for FakeMixer {
    fn default() -> Self {
        Self {
            range: (0, 87),
            raw: 60,
            switch: Some(true),
            fail: false,
        }
    }
}

impl PlaybackMixer for FakeMixer {
    fn volume_range(&self) -> io::Result<(i64, i64)> {
        Ok(self.range)
    }

    fn volume(&self) -> io::Result<i64> {
        Ok(self.raw)
    }

    fn set_volume(&mut self, raw: i64) -> io::Result<()> {
        if self.fail {
            return Err(io::Error::other("mixer write failed"));
        }
        self.raw = raw;
        Ok(())
    }

    fn has_switch(&self) -> bool {
        self.switch.is_some()
    }

    fn switch_on(&self) -> io::Result<bool> {
        Ok(self.switch.expect("a playback switch"))
    }

    fn set_switch(&mut self, on: bool) -> io::Result<()> {
        *self.switch.as_mut().expect("a playback switch") = on;
        Ok(())
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    state: PathBuf,
    sys_root: PathBuf,
    calls: Vec<Invocation>,
    command_fails: bool,
    mixer: FakeMixer,
    volume_actions: Vec<VolumeAction>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("run");
        let sys_root = temp.path().join("sys");
        fs::create_dir(&state).unwrap();
        Self {
            temp,
            state,
            sys_root,
            calls: Vec::new(),
            command_fails: false,
            mixer: FakeMixer::default(),
            volume_actions: Vec::new(),
        }
    }

    fn backlight(&self, name: &str, now: i64, top: i64) {
        let path = self.sys_root.join("class/backlight").join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("brightness"), format!("{now}\n")).unwrap();
        fs::write(path.join("max_brightness"), format!("{top}\n")).unwrap();
    }

    fn run(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> (u8, Vec<u8>) {
        let mut stderr = Vec::new();
        let mut keys = MediaKeys {
            state: &self.state,
            sys_root: &self.sys_root,
            run_command: |command: &mut Command, timeout| {
                self.calls.push(Invocation {
                    program: command.get_program().to_owned(),
                    args: command.get_args().map(OsStr::to_owned).collect(),
                    env: command
                        .get_envs()
                        .map(|(key, value)| (key.to_owned(), value.map(OsStr::to_owned)))
                        .collect(),
                    timeout,
                });
                if self.command_fails {
                    return Err(io::Error::other("brightnessctl failed"));
                }
                Ok(())
            },
            change_volume: |action| {
                self.volume_actions.push(action);
                change_volume(action, &mut self.mixer)
            },
        };
        let code = keys.run(args, &mut stderr);
        (code, stderr)
    }

    fn press(&mut self, action: &str) -> u8 {
        let (code, stderr) = self.run([action]);
        assert!(stderr.is_empty());
        code
    }

    fn last_brightness_target(&self) -> &OsStr {
        self.calls.last().unwrap().args.last().unwrap()
    }
}

#[test]
fn brightness_moves_one_step_on_the_screens_backlight_and_tells_the_app() {
    let mut fixture = Fixture::new();
    fixture.backlight("intel_backlight", 9600, 19200);
    assert_eq!(fixture.press("brightness-up"), 0);
    let call = &fixture.calls[0];
    assert_eq!(call.program, OsString::from(BRIGHTNESSCTL));
    assert_eq!(
        call.args,
        [
            "--quiet",
            "--class=backlight",
            "--device=intel_backlight",
            "set",
            "10560",
        ]
        .map(OsString::from)
    );
    assert_eq!(
        call.env,
        BTreeMap::from([
            ("PATH".into(), Some("/usr/bin:/bin".into())),
            ("LANG".into(), Some("C.UTF-8".into())),
        ])
    );
    assert_eq!(call.timeout, Duration::from_secs(5));
    assert!(fs::read(fixture.state.join("changed")).unwrap().is_empty());

    assert_eq!(fixture.press("brightness-down"), 0);
    assert_eq!(fixture.last_brightness_target(), OsStr::new("8640"));
}

#[test]
fn brightness_never_goes_dark_or_past_the_top() {
    let mut fixture = Fixture::new();
    fixture.backlight("acpi_video0", 1, 100);
    assert_eq!(fixture.press("brightness-down"), 0);
    assert_eq!(fixture.last_brightness_target(), OsStr::new("5"));
    fixture.backlight("acpi_video0", 98, 100);
    assert_eq!(fixture.press("brightness-up"), 0);
    assert_eq!(fixture.last_brightness_target(), OsStr::new("100"));
}

#[test]
fn at_the_limit_nothing_runs_and_the_app_is_not_told() {
    let mut fixture = Fixture::new();
    fixture.backlight("acpi_video0", 100, 100);
    assert_eq!(fixture.press("brightness-up"), 0);
    assert!(fixture.calls.is_empty());
    assert!(!fixture.state.join("changed").exists());
}

#[test]
fn a_coarse_backlight_still_moves_one_hardware_step() {
    assert_eq!(brightness_target(4, 7, true), 5);
    assert_eq!(brightness_target(4, 7, false), 3);
    assert_eq!(brightness_target(1, 7, false), 1);
}

#[test]
fn brightness_target_rounds_steps_to_even_and_ceilings_the_floor() {
    assert_eq!(brightness_target(10, 50, true), 12);
    assert_eq!(brightness_target(10, 70, true), 14);
    assert_eq!(brightness_target(3, 41, false), 3);
}

#[test]
fn no_backlight_is_not_an_error() {
    let mut fixture = Fixture::new();
    assert_eq!(fixture.press("brightness-up"), 0);
    assert!(fixture.calls.is_empty());
    assert!(!fixture.state.join("changed").exists());
}

#[test]
fn unreadable_backlights_are_skipped_for_the_next() {
    let mut fixture = Fixture::new();
    fixture.backlight("c_other", 60, 100);
    fixture.backlight("a_broken", 5, 0);
    fixture.backlight("b_good", 50, 100);
    assert_eq!(fixture.press("brightness-up"), 0);
    assert_eq!(
        fixture.calls[0].args[2].as_os_str(),
        OsStr::new("--device=b_good")
    );
}

#[test]
fn volume_keys_go_to_the_mixer() {
    let mut fixture = Fixture::new();
    for action in ["volume-up", "volume-down", "mute"] {
        assert_eq!(fixture.press(action), 0);
    }
    assert_eq!(
        fixture.volume_actions,
        [VolumeAction::Up, VolumeAction::Down, VolumeAction::Toggle]
    );
    assert!(fixture.calls.is_empty());
    assert!(fs::read(fixture.state.join("changed")).unwrap().is_empty());
}

#[test]
fn up_and_down_move_one_step_and_up_unmutes() {
    let mut mixer = FakeMixer {
        switch: Some(false),
        ..FakeMixer::default()
    };
    change_volume(VolumeAction::Up, &mut mixer).unwrap();
    assert_eq!((mixer.raw, mixer.switch), (64, Some(true)));
    change_volume(VolumeAction::Down, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 60);
    mixer.switch = Some(false);
    change_volume(VolumeAction::Down, &mut mixer).unwrap();
    assert_eq!((mixer.raw, mixer.switch), (56, Some(false)));
}

#[test]
fn up_and_down_stop_at_the_ends() {
    let mut mixer = FakeMixer {
        raw: 87,
        ..FakeMixer::default()
    };
    change_volume(VolumeAction::Up, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 87);
    mixer.raw = 1;
    change_volume(VolumeAction::Down, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 0);
    change_volume(VolumeAction::Down, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 0);
}

#[test]
fn a_coarse_mixer_still_moves_one_hardware_step() {
    let mut mixer = FakeMixer {
        range: (10, 17),
        raw: 11,
        switch: None,
        ..FakeMixer::default()
    };
    change_volume(VolumeAction::Up, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 12);
    change_volume(VolumeAction::Down, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 11);
}

#[test]
fn volume_rounds_half_steps_to_even() {
    let mut mixer = FakeMixer {
        range: (0, 50),
        raw: 10,
        ..FakeMixer::default()
    };
    change_volume(VolumeAction::Up, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 12);
}

#[test]
fn toggle_flips_the_mute_switch() {
    let mut mixer = FakeMixer::default();
    change_volume(VolumeAction::Toggle, &mut mixer).unwrap();
    assert_eq!((mixer.raw, mixer.switch), (60, Some(false)));
    change_volume(VolumeAction::Toggle, &mut mixer).unwrap();
    assert_eq!((mixer.raw, mixer.switch), (60, Some(true)));
}

#[test]
fn a_card_without_a_mute_switch_mutes_at_its_minimum() {
    let mut mixer = FakeMixer {
        switch: None,
        ..FakeMixer::default()
    };
    change_volume(VolumeAction::Toggle, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 0);
    mixer.range = (10, 87);
    mixer.raw = 60;
    change_volume(VolumeAction::Toggle, &mut mixer).unwrap();
    assert_eq!(mixer.raw, 10);
}

#[test]
fn music_keys_are_logged_for_the_app() {
    let mut fixture = Fixture::new();
    let log = fixture.state.join("music");
    assert_eq!(fixture.press("play-pause"), 0);
    assert_eq!(fs::read_to_string(&log).unwrap(), "1 play-pause\n");
    for _ in 0..2 {
        assert_eq!(fixture.press("next"), 0);
    }
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "1 play-pause\n2 next\n3 next\n"
    );
    assert_eq!(fs::metadata(log).unwrap().permissions().mode() & 0o7777, 0o644);
    assert!(fixture.calls.is_empty());
    assert!(fixture.volume_actions.is_empty());
    assert!(!fixture.state.join("changed").exists());
}

#[test]
fn the_music_log_keeps_the_last_eight() {
    let mut fixture = Fixture::new();
    for _ in 0..10 {
        assert_eq!(fixture.press("previous"), 0);
    }
    assert_eq!(
        fs::read_to_string(fixture.state.join("music")).unwrap(),
        "3 previous\n4 previous\n5 previous\n6 previous\n\
         7 previous\n8 previous\n9 previous\n10 previous\n"
    );
}

#[test]
fn a_garbled_log_starts_again_at_one() {
    let mut fixture = Fixture::new();
    let log = fixture.state.join("music");
    for garbled in [
        "40 previous\n41 junk\n",
        "0 next\n",
        "+41 next\n",
        "\u{661} next\n",
        "41 next extra\n",
        "41 next\n\n",
    ] {
        fs::write(&log, garbled).unwrap();
        assert_eq!(fixture.press("play-pause"), 0);
        assert_eq!(fs::read_to_string(&log).unwrap(), "1 play-pause\n");
    }
    fs::write(&log, b"\xff\n41 next\n").unwrap();
    assert_eq!(fixture.press("play-pause"), 0);
    assert_eq!(fs::read_to_string(&log).unwrap(), "1 play-pause\n");
}

#[test]
fn music_sequence_can_reach_u64_max_and_then_starts_over() {
    let mut fixture = Fixture::new();
    let log = fixture.state.join("music");
    fs::write(&log, "18446744073709551614 previous\n").unwrap();
    assert_eq!(fixture.press("next"), 0);
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "18446744073709551614 previous\n18446744073709551615 next\n"
    );
    assert_eq!(fixture.press("play-pause"), 0);
    assert_eq!(fs::read_to_string(&log).unwrap(), "1 play-pause\n");
}

#[test]
fn only_the_last_music_line_is_validated_and_separators_are_normalized() {
    let mut fixture = Fixture::new();
    let log = fixture.state.join("music");
    fs::write(&log, "junk\r\n \x1f0040\u{a0}previous \r").unwrap();
    assert_eq!(fixture.press("next"), 0);
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "junk\n \x1f0040\u{a0}previous \n41 next\n"
    );
}

#[test]
fn music_keys_take_a_blocking_lock() {
    let fixture = Fixture::new();
    let state = fixture.state.as_path();
    let lock = open_lock(state).unwrap();
    lock.lock().unwrap();
    thread::scope(|scope| {
        let (starting, started) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        scope.spawn(move || {
            starting.send(()).unwrap();
            done.send(record_music_key(state, "next")).unwrap();
        });
        let started = started.recv();
        let early = completed.recv_timeout(Duration::from_millis(50));
        drop(lock);
        started.unwrap();
        assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
        completed.recv().unwrap().unwrap();
    });
    assert_eq!(fs::read_to_string(state.join("music")).unwrap(), "1 next\n");
}

#[test]
fn a_press_while_the_last_still_runs_is_dropped() {
    let mut fixture = Fixture::new();
    let lock = open_lock(&fixture.state).unwrap();
    lock.lock().unwrap();
    fixture.backlight("intel_backlight", 50, 100);
    assert_eq!(fixture.press("brightness-up"), 0);
    assert_eq!(fixture.press("volume-up"), 0);
    assert!(fixture.calls.is_empty());
    assert!(fixture.volume_actions.is_empty());
    assert!(!fixture.state.join("changed").exists());
}

#[test]
fn a_failed_change_reports_failure_and_does_not_tell_the_app() {
    let mut fixture = Fixture::new();
    fixture.backlight("intel_backlight", 50, 100);
    fixture.command_fails = true;
    assert_eq!(fixture.press("brightness-up"), 1);
    assert!(!fixture.state.join("changed").exists());
    fixture.mixer.fail = true;
    assert_eq!(fixture.press("volume-up"), 1);
    assert!(!fixture.state.join("changed").exists());
}

#[test]
fn without_its_state_directory_it_still_works() {
    let mut fixture = Fixture::new();
    fixture.backlight("intel_backlight", 50, 100);
    fixture.state = fixture.temp.path().join("missing");
    assert_eq!(fixture.press("volume-up"), 0);
    assert_eq!(fixture.volume_actions, [VolumeAction::Up]);
    assert_eq!(fixture.press("next"), 1);
}

#[test]
fn refuses_anything_else() {
    let mut fixture = Fixture::new();
    let usage: &[u8] = b"usage: wros-media-key brightness-up | brightness-down | volume-up | \
                        volume-down | mute | play-pause | next | previous\n";
    for bad in [
        &[][..],
        &["brightness"],
        &["volume-up", "extra"],
        &["set", "50"],
        &["--help"],
    ] {
        let (code, stderr) = fixture.run(bad);
        assert_eq!(code, 2);
        assert_eq!(stderr.as_slice(), usage);
    }
    let (code, stderr) = fixture.run([OsString::from_vec(vec![0xff])]);
    assert_eq!(code, 2);
    assert_eq!(stderr.as_slice(), usage);
    assert!(fixture.calls.is_empty());
    assert!(fixture.volume_actions.is_empty());
}
