#![cfg(target_os = "linux")]

use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn the_message_mode_finishes_without_reading_input_or_contacting_greetd() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wros-greeter"))
        .arg("--lock-message")
        .env_clear()
        .env("LANG", "C.UTF-8")
        .env("GREETD_SOCK", "/nonexistent/wros-test-no-greetd.sock")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    // Keep stdin open without supplying a byte: presentation must not wait on it.
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("the message mode waited on input or the login service");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let reply = child.wait_with_output().unwrap();
    assert!(
        reply.status.success(),
        "the presentation command must work without greetd"
    );
    assert!(reply.stderr.is_empty());
    let message = String::from_utf8(reply.stdout).unwrap();
    assert!(message.contains("Locked at ") && message.contains(['▀', '▄', '█']));
    assert!(message.ends_with("Press Enter to unlock."));
}
