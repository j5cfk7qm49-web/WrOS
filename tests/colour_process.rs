//! The colour owner keeps its deadline when the writing process cannot run.
#![cfg(target_os = "linux")]
#[path = "support/gamma.rs"]
mod fixture;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const BINARY: &str = env!("CARGO_BIN_EXE_grimoire");

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn preview_expires_while_its_parent_is_stopped_and_pipe_loss_ends_the_colour_owner() {
    let server = fixture::Server::start("positive");
    let helper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("os/tests/gamma-frozen-parent.py");
    let child = Command::new("python3")
        .arg(helper)
        .arg(BINARY)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &server.dir)
        .env("XDG_RUNTIME_DIR", &server.dir)
        .env("WAYLAND_DISPLAY", "gamma-test")
        .env("WROS_COMPOSITOR_WARMTH", "ready")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut parent = Running(child);
    let mut line = String::new();
    BufReader::new(parent.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let announced: serde_json::Value = serde_json::from_str(&line).expect("colour worker ready");
    let worker = announced["worker"].as_u64().unwrap();
    let state =
        || fs::read_to_string(format!("/proc/{}/status", parent.0.id())).unwrap_or_default();
    let end = Instant::now() + Duration::from_secs(2);
    while !state().contains("State:\tT") {
        assert!(Instant::now() < end, "the fixture parent was not stopped");
        std::thread::sleep(Duration::from_millis(10));
    }
    let end = Instant::now() + Duration::from_secs(18);
    loop {
        let log = server.log();
        let last_set = log.rfind("set 1").expect("both outputs were warmed");
        let restored = &log[last_set..];
        if restored.contains("restore 0\n") && restored.contains("restore 1\n") {
            break;
        }
        assert!(
            Instant::now() < end,
            "preview outlived its independent deadline: {log}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        state().contains("State:\tT"),
        "the parent resumed before rollback"
    );
    for fd in fs::read_dir(format!("/proc/{worker}/fd"))
        .unwrap()
        .flatten()
    {
        let target = fs::read_link(fd.path()).unwrap_or_default();
        let target = target.to_string_lossy();
        assert!(!target.starts_with("/dev/input/") && !target.starts_with("/dev/tty"));
    }
    parent.0.kill().unwrap();
    parent.0.wait().unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let status = fs::read_to_string(format!("/proc/{worker}/status")).unwrap_or_default();
        if status.is_empty() || status.contains("State:\tZ") {
            break;
        }
        assert!(
            Instant::now() < end,
            "colour owner survived its closed request pipe"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn oversized_or_unrelated_commands_cannot_write_curves_or_request_power_actions() {
    let server = fixture::Server::start("positive");
    for input in ["x".repeat(4097), "{\"Power\":\"Shutdown\"}\n".into()] {
        let mut worker = Running(
            Command::new(BINARY)
                .arg("--wros-colour-worker")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", &server.dir)
                .env("XDG_RUNTIME_DIR", &server.dir)
                .env("WAYLAND_DISPLAY", "gamma-test")
                .env("WROS_COMPOSITOR_WARMTH", "ready")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        worker
            .0
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(exit) = worker.0.try_wait().unwrap() {
                assert!(!exit.success());
                break;
            }
            assert!(Instant::now() < end, "invalid colour request did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(!server.log().contains("set "));
}
