use super::{Screen, Transaction, protocol::Protocol};
use std::{
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
use wros_sys::display::Warmth;
#[path = "../../tests/support/gamma.rs"]
mod fixture;
use fixture::Server;
fn connect(server: &Server) -> anyhow::Result<Protocol> {
    Protocol::from_socket(UnixStream::connect(server.dir.join("gamma-test"))?)
}

#[test]
fn real_protocol_sets_both_outputs_and_off_destroys_controls_instead_of_writing_identity() {
    let server = Server::start("positive");
    let mut screen = connect(&server).unwrap();
    screen.apply(Warmth::default()).unwrap();
    let warm_log = server.log();
    assert_eq!(
        warm_log
            .lines()
            .filter(|line| line.starts_with("set "))
            .count(),
        2
    );
    for line in warm_log.lines().filter(|line| line.starts_with("set ")) {
        let values: Vec<u16> = line
            .split_whitespace()
            .skip(2)
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(values[0], 65535);
        assert!(values[1] < values[0] && values[2] < values[1]);
    }
    screen
        .apply(Warmth {
            enabled: false,
            ..Warmth::default()
        })
        .unwrap();
    let log = server.log();
    assert_eq!(
        log.lines().filter(|line| line.starts_with("set ")).count(),
        2
    );
    assert!(log.ends_with("restore 0\nrestore 1\n"));
}

#[test]
fn unsupported_missing_and_oversized_protocols_never_set_a_curve() {
    for mode in ["absent", "unsupported", "oversized", "missing-size"] {
        let server = Server::start(mode);
        assert!(connect(&server).is_err(), "{mode}");
        assert!(!server.log().contains("set "), "{mode}");
    }
}

#[test]
fn partial_failure_hotplug_and_disconnect_release_all_outputs() {
    for mode in ["partial", "remove", "disconnect"] {
        let server = Server::start(mode);
        let mut screen = connect(&server).unwrap();
        assert!(screen.apply(Warmth::default()).is_err(), "{mode}");
        drop(screen);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !server.log().ends_with("restore 0\nrestore 1\n") && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let log = server.log();
        let after_write = &log[log.find("set ").unwrap()..];
        assert!(
            after_write.contains("restore 0\n") && after_write.contains("restore 1\n"),
            "{mode}: {log}"
        );
    }
}

#[test]
fn a_real_protocol_preview_reverts_without_any_writer_or_system_request() {
    let server = Server::start("positive");
    let mut trial = Transaction {
        screen: connect(&server).unwrap(),
        saved: Warmth {
            enabled: false,
            ..Warmth::default()
        },
        pending: None,
    };
    trial
        .preview(Warmth::default(), "a".repeat(32), Instant::now())
        .unwrap();
    // Use the production poll/deadline path, with a shortened test-only deadline.
    let deadline = Instant::now() + Duration::from_millis(60);
    trial.pending.as_mut().unwrap().deadline = deadline;
    trial.screen.wait(-1, Some(deadline)).unwrap();
    trial.expire(Instant::now()).unwrap();
    assert!(trial.pending.is_none());
    assert!(server.log().ends_with("restore 0\nrestore 1\n"));
}
