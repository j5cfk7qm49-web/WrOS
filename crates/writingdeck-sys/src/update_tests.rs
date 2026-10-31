//! End-to-end adapter tests for the update verbs. Each verb runs a real fake
//! helper process (a small sh script) through the real `ProcessRunner`, so
//! exit codes, stdout/stderr limits and timeouts are the production code paths.
use crate::linux::{Invocation, Linux, ProcessRunner, Runner};
use crate::{Backend, Reply, Request, UpdateState};
use anyhow::Result;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Runs `/usr/libexec/<name>` (directly, or as the target of
/// `pkexec --disable-internal-agent`) as `/bin/sh <dir>/<name> args...`.
struct Fakes {
    dir: PathBuf,
    seen: Vec<Invocation>,
    limit: Option<Duration>,
}

impl Fakes {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "wros-sys-update-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            seen: vec![],
            limit: None,
        }
    }
    fn helper(&self, name: &str, body: &str) {
        fs::write(self.dir.join(name), format!("{body}\n")).unwrap();
    }
    fn backend(self) -> Linux<Self> {
        Linux {
            runner: self,
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        }
    }
}

impl Drop for Fakes {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Runner for Fakes {
    fn run(&mut self, cmd: &Invocation) -> Result<String> {
        self.seen.push(cmd.clone());
        let (helper, args) = if cmd.program == "/usr/bin/pkexec" {
            assert_eq!(cmd.args[0], "--disable-internal-agent");
            (cmd.args[1].as_str(), &cmd.args[2..])
        } else {
            (cmd.program, &cmd.args[..])
        };
        let name = helper.rsplit('/').next().unwrap();
        let mut argv = vec![self.dir.join(name).to_string_lossy().into_owned()];
        argv.extend(args.iter().cloned());
        let real = Invocation {
            program: "/bin/sh",
            args: argv,
            timeout: self.limit.unwrap_or(cmd.timeout),
        };
        ProcessRunner.run(&real)
    }
}

const DIR: &str = "/media/usb/wros-updates";

fn id() -> String {
    "0123456789abcdef".repeat(4)
}

fn review_json() -> String {
    format!(
        r#"{{"status":"reviewed","review_id":"{}","signer":"{}","suite":"trixie-wros","date":"2026-10-31","valid_until":"2026-12-31","packages":[{{"name":"wros","from_version":"0.1.0~rc1","to_version":"0.1.0~rc2","action":"upgrade"}},{{"name":"wros-help","from_version":null,"to_version":"1:2.0-1","action":"install"}}],"removals":["old-wros-tool"],"backup_recommended":true}}"#,
        id(),
        "A".repeat(40)
    )
}

fn review_request() -> Request {
    Request::ReviewUpdateBundle {
        bundle_dir: DIR.into(),
    }
}

fn stage_request() -> Request {
    Request::StageUpdates {
        bundle_dir: DIR.into(),
        review_id: id(),
    }
}

fn echo(json: &str) -> String {
    format!("cat <<'JSON'\n{json}\nJSON")
}

#[test]
fn review_success_is_parsed_strictly_and_runs_unprivileged() {
    let fakes = Fakes::new();
    fakes.helper("wros-update", &echo(&review_json()));
    let mut backend = fakes.backend();
    let Reply::UpdateReview(review) = backend.request(review_request()).unwrap() else {
        panic!("expected a review");
    };
    assert_eq!(review.review_id, id());
    assert_eq!(review.signer, "A".repeat(40));
    assert_eq!(review.valid_until, "2026-12-31");
    assert_eq!(review.packages.len(), 2);
    assert_eq!(
        review.packages[0].from_version.as_deref(),
        Some("0.1.0~rc1")
    );
    assert_eq!(review.packages[0].action, "upgrade");
    assert_eq!(review.packages[1].from_version, None);
    assert_eq!(review.packages[1].to_version, "1:2.0-1");
    assert_eq!(review.removals, ["old-wros-tool"]);
    assert!(review.backup_recommended);
    let seen = &backend.runner.seen[0];
    assert_eq!(seen.program, "/usr/libexec/wros-update");
    assert_eq!(seen.args, ["review", DIR]);
}

#[test]
fn stage_success_requires_the_reviewed_identity_to_be_echoed() {
    let fakes = Fakes::new();
    // $1 is the bundle directory and $2 the review identity the writer approved.
    fakes.helper(
        "wros-update-stage",
        r#"printf '{"status":"staged","transaction_id":"tx-2026.1","review_id":"%s","packages":2,"restart_required":true}\n' "$2""#,
    );
    let mut backend = fakes.backend();
    let Reply::UpdateStaged(staged) = backend.request(stage_request()).unwrap() else {
        panic!("expected staged");
    };
    assert_eq!(staged.review_id, id());
    assert_eq!(staged.transaction_id, "tx-2026.1");
    assert_eq!(staged.packages, 2);
    assert!(staged.restart_required);
    let seen = &backend.runner.seen[0];
    assert_eq!(seen.program, "/usr/bin/pkexec");
    assert_eq!(
        seen.args,
        [
            "--disable-internal-agent",
            "/usr/libexec/wros-update-stage",
            DIR,
            id().as_str()
        ]
    );

    // A helper that staged something else must not be reported as success.
    let other = Fakes::new();
    other.helper(
        "wros-update-stage",
        &echo(&format!(
            r#"{{"status":"staged","transaction_id":"tx-1","review_id":"{}","packages":1,"restart_required":true}}"#,
            "f".repeat(64)
        )),
    );
    let error = other
        .backend()
        .request(stage_request())
        .unwrap_err()
        .to_string();
    assert!(error.contains("different update"), "{error}");
}

#[test]
fn malformed_replies_are_rejected_for_every_verb() {
    let cases: Vec<(&str, Request)> = vec![
        ("wros-update", review_request()),
        ("wros-update-stage", stage_request()),
        ("wros-update-cancel", Request::CancelStagedUpdates),
        ("wros-update-repair", Request::RecoverUpdates),
        ("wros-update-media", Request::FindUpdateMedia),
        ("wros-update", Request::RecoveryStatus),
    ];
    for output in [
        "not json at all",
        "[]",
        "{}",
        r#"{"status":"unknown-state","message":"x"}"#,
        r#"{"status":42}"#,
        "",
    ] {
        for (name, request) in &cases {
            let fakes = Fakes::new();
            fakes.helper(name, &echo(output));
            assert!(
                fakes.backend().request(request.clone()).is_err(),
                "{name} accepted {output:?}"
            );
        }
    }
}

#[test]
fn reviews_with_unknown_fields_or_inconsistent_content_are_refused() {
    let good = review_json();
    let bad = [
        good.replace(
            r#""backup_recommended":true"#,
            r#""backup_recommended":true,"extra":1"#,
        ),
        good.replace(&id(), &"A".repeat(64)),
        good.replace(&id(), &"a".repeat(63)),
        good.replace(&"A".repeat(40), &"A".repeat(39)),
        good.replace(&"A".repeat(40), &"Z".repeat(40)),
        good.replace(r#""status":"reviewed""#, r#""status":"staged""#),
        // install must have no previous version; upgrade must have one.
        good.replace(r#""from_version":null"#, r#""from_version":"1.0""#),
        good.replace(r#""from_version":"0.1.0~rc1""#, r#""from_version":null"#),
        good.replace(r#""action":"upgrade""#, r#""action":"downgrade""#),
        good.replace(r#""name":"wros-help""#, r#""name":"wros""#),
        good.replace(r#""name":"wros-help""#, r#""name":"Bad Name""#),
        good.replace(r#""to_version":"1:2.0-1""#, r#""to_version":"1 2""#),
        good.replace(r#""removals":["old-wros-tool"]"#, r#""removals":["wros"]"#),
        good.replace(r#""date":"2026-10-31""#, r#""date":"a\u0007b""#),
        good.replace(r#""valid_until":"2026-12-31""#, r#""valid_until":"""#),
    ];
    for (n, output) in bad.iter().enumerate() {
        assert_ne!(output, &good, "mutation {n} changed nothing");
        let fakes = Fakes::new();
        fakes.helper("wros-update", &echo(output));
        assert!(
            fakes.backend().request(review_request()).is_err(),
            "mutation {n} accepted: {output}"
        );
    }
}

#[test]
fn failed_status_with_exit_zero_and_nonzero_exit_both_carry_the_helper_message() {
    for (name, request) in [
        ("wros-update-cancel", Request::CancelStagedUpdates),
        ("wros-update-repair", Request::RecoverUpdates),
        ("wros-update-media", Request::FindUpdateMedia),
        ("wros-update-stage", stage_request()),
        ("wros-update", review_request()),
    ] {
        let fakes = Fakes::new();
        fakes.helper(
            name,
            &echo(r#"{"status":"failed","message":"The signature check failed."}"#),
        );
        let error = fakes.backend().request(request.clone()).unwrap_err();
        assert!(
            error.to_string().contains("signature check failed"),
            "{name}"
        );

        // Non-zero exit with the explanation only on stdout.
        let fakes = Fakes::new();
        fakes.helper(
            name,
            &format!(
                "{}\nexit 3",
                echo(r#"{"status":"failed","message":"Disk is read-only."}"#)
            ),
        );
        let error = fakes.backend().request(request.clone()).unwrap_err();
        assert!(error.to_string().contains("Disk is read-only"), "{name}");

        // Non-zero exit with stderr (pkexec authorization failures look like this).
        let fakes = Fakes::new();
        fakes.helper(name, "echo 'Not authorized' >&2\nexit 126");
        let error = fakes.backend().request(request).unwrap_err();
        assert!(error.to_string().contains("Not authorized"), "{name}");
    }

    // No output at all: the exit status is the only truthful detail.
    let fakes = Fakes::new();
    fakes.helper("wros-update-cancel", "exit 5");
    let error = fakes
        .backend()
        .request(Request::CancelStagedUpdates)
        .unwrap_err();
    assert!(error.to_string().contains("exit"), "{error}");
}

#[test]
fn oversize_output_is_never_a_partial_result() {
    for (name, request) in [
        ("wros-update", review_request()),
        ("wros-update-stage", stage_request()),
        ("wros-update-cancel", Request::CancelStagedUpdates),
        ("wros-update-repair", Request::RecoverUpdates),
        ("wros-update-media", Request::FindUpdateMedia),
    ] {
        // A valid first line followed by more than the 64 KiB cap.
        let fakes = Fakes::new();
        fakes.helper(
            name,
            &format!(
                "{}\nhead -c 70000 /dev/zero | tr '\\000' x\necho",
                echo(&review_json())
            ),
        );
        let error = fakes.backend().request(request).unwrap_err().to_string();
        assert!(error.contains("exceeds limit"), "{name}: {error}");
    }
}

#[test]
fn a_hung_helper_times_out_without_success_and_without_hanging_the_worker() {
    let mut fakes = Fakes::new();
    fakes.limit = Some(Duration::from_millis(600));
    // The background sleeper keeps stdout open after the launcher is killed.
    fakes.helper("wros-update-stage", "sleep 6 &\nexec sleep 6");
    let mut backend = fakes.backend();
    let started = Instant::now();
    let error = backend.request(stage_request()).unwrap_err().to_string();
    assert!(error.contains("timed out"), "{error}");
    assert!(error.contains("no success assumed"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the worker waited for the orphaned helper: {:?}",
        started.elapsed()
    );
}

#[test]
fn cancel_and_repair_map_every_documented_status() {
    for (verb, request, status, message) in [
        (
            "wros-update-cancel",
            Request::CancelStagedUpdates,
            "cancelled",
            "Staged update cancelled; nothing was installed.",
        ),
        (
            "wros-update-cancel",
            Request::CancelStagedUpdates,
            "none",
            "No staged update.",
        ),
        (
            "wros-update-repair",
            Request::RecoverUpdates,
            "recovered",
            "Repaired from retained archives.",
        ),
        (
            "wros-update-repair",
            Request::RecoverUpdates,
            "clean",
            "Nothing needed repair.",
        ),
    ] {
        let fakes = Fakes::new();
        fakes.helper(
            verb,
            &echo(&format!(r#"{{"status":"{status}","message":"{message}"}}"#)),
        );
        let mut backend = fakes.backend();
        assert_eq!(
            backend.request(request).unwrap(),
            Reply::Message(message.into())
        );
        let seen = &backend.runner.seen[0];
        assert_eq!(seen.program, "/usr/bin/pkexec");
        assert_eq!(seen.args[0], "--disable-internal-agent");
    }
    // Statuses that belong to a different verb are not accepted.
    for (verb, request, status) in [
        (
            "wros-update-cancel",
            Request::CancelStagedUpdates,
            "recovered",
        ),
        ("wros-update-cancel", Request::CancelStagedUpdates, "staged"),
        ("wros-update-repair", Request::RecoverUpdates, "cancelled"),
        ("wros-update-repair", Request::RecoverUpdates, "mounted"),
    ] {
        let fakes = Fakes::new();
        fakes.helper(
            verb,
            &echo(&format!(r#"{{"status":"{status}","message":"m"}}"#)),
        );
        assert!(fakes.backend().request(request).is_err(), "{verb} {status}");
    }
    // A reply without the message is incomplete.
    let fakes = Fakes::new();
    fakes.helper("wros-update-cancel", &echo(r#"{"status":"cancelled"}"#));
    assert!(
        fakes
            .backend()
            .request(Request::CancelStagedUpdates)
            .is_err()
    );
}

#[test]
fn media_mount_reports_only_managed_mount_points() {
    let mounted = r#"{"status":"mounted","path":"/run/wros-update/media/0"}"#;
    let fakes = Fakes::new();
    fakes.helper("wros-update-media", &echo(mounted));
    let mut backend = fakes.backend();
    assert_eq!(
        backend.request(Request::FindUpdateMedia).unwrap(),
        Reply::UpdateMedia(Some("/run/wros-update/media/0".into()))
    );
    assert_eq!(
        backend.runner.seen[0].args,
        [
            "--disable-internal-agent",
            "/usr/libexec/wros-update-media",
            "mount"
        ]
    );
    let fakes = Fakes::new();
    fakes.helper("wros-update-media", &echo(r#"{"status":"none"}"#));
    assert_eq!(
        fakes.backend().request(Request::FindUpdateMedia).unwrap(),
        Reply::UpdateMedia(None)
    );
    for output in [
        r#"{"status":"mounted","path":"/etc"}"#,
        r#"{"status":"mounted","path":"/media/usb"}"#,
        r#"{"status":"mounted","path":"/run/wros-update/media/../../x"}"#,
        r#"{"status":"mounted","path":"relative"}"#,
        r#"{"status":"mounted"}"#,
        r#"{"status":"none","path":"/run/wros-update/media/0"}"#,
        r#"{"status":"mounted","path":"/run/wros-update/media/0","extra":1}"#,
    ] {
        let fakes = Fakes::new();
        fakes.helper("wros-update-media", &echo(output));
        assert!(
            fakes.backend().request(Request::FindUpdateMedia).is_err(),
            "{output}"
        );
    }
}

#[test]
fn recovery_status_carries_the_journal_state_and_rejects_unknown_ones() {
    let status = |state: &str| {
        format!(
            r#"{{"root_filesystem":"ext4","root_device":"/dev/mapper/root","separate_home":false,"rollback_capable":false,"rollback_strategy":"ext4 has no snapshot rollback","interrupted_dpkg":false,"unconfigured_packages":[],"staged_update":true,"staged_packages":["wros"],"update_state":"{state}","status":"ok"}}"#
        )
    };
    for (name, state) in [
        ("idle", UpdateState::Idle),
        ("staged", UpdateState::Staged),
        ("applying", UpdateState::Applying),
        ("failed", UpdateState::Failed),
        ("recovered", UpdateState::Recovered),
        ("completed", UpdateState::Completed),
        ("interrupted", UpdateState::Interrupted),
    ] {
        let fakes = Fakes::new();
        fakes.helper("wros-update", &echo(&status(name)));
        let Reply::Recovery(info) = fakes.backend().request(Request::RecoveryStatus).unwrap()
        else {
            panic!("expected recovery");
        };
        assert_eq!(info.update_state, state);
        assert_eq!(info.staged_packages, ["wros"]);
    }
    for output in [
        status("rolled-back"),
        status("STAGED"),
        status("staged").replace(r#""update_state":"staged","#, ""),
        status("staged").replace(r#""status":"ok""#, r#""status":"ok","extra":true"#),
    ] {
        let fakes = Fakes::new();
        fakes.helper("wros-update", &echo(&output));
        assert!(
            fakes.backend().request(Request::RecoveryStatus).is_err(),
            "{output}"
        );
    }
}

#[test]
fn simulated_backend_models_review_stage_cancel_and_repair_with_labelled_text() {
    let mut sim = crate::Simulated::laptop();
    let Reply::UpdateMedia(Some(media)) = sim.request(Request::FindUpdateMedia).unwrap() else {
        panic!("simulated media");
    };
    let Reply::UpdateReview(review) = sim
        .request(Request::ReviewUpdateBundle {
            bundle_dir: media.clone(),
        })
        .unwrap()
    else {
        panic!("simulated review");
    };
    assert!(review.signer.contains("SIMULATED"));
    assert!(review.suite.contains("SIMULATED"));
    // Refusals mirror the real adapter and nothing is staged by them.
    assert!(
        sim.request(Request::StageUpdates {
            bundle_dir: media.clone(),
            review_id: "short".into(),
        })
        .is_err()
    );
    assert!(
        sim.request(Request::StageUpdates {
            bundle_dir: "/etc".into(),
            review_id: review.review_id.clone(),
        })
        .is_err()
    );
    assert_eq!(sim.recovery.update_state, UpdateState::Idle);
    // Media that changed after review is refused.
    sim.bundle_revision += 1;
    let error = sim
        .request(Request::StageUpdates {
            bundle_dir: media.clone(),
            review_id: review.review_id.clone(),
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("SIMULATED") && error.contains("no longer matches"));
    assert_eq!(sim.recovery.update_state, UpdateState::Idle);
    // The re-reviewed identity stages, and cancel returns to idle.
    let again = sim.review_id();
    let Reply::UpdateStaged(staged) = sim
        .request(Request::StageUpdates {
            bundle_dir: media,
            review_id: again.clone(),
        })
        .unwrap()
    else {
        panic!("simulated stage");
    };
    assert_eq!(staged.review_id, again);
    assert_eq!(sim.recovery.update_state, UpdateState::Staged);
    assert!(sim.recovery.staged_update);
    let Reply::Message(cancelled) = sim.request(Request::CancelStagedUpdates).unwrap() else {
        panic!("simulated cancel");
    };
    assert!(cancelled.starts_with("SIMULATED"));
    assert_eq!(sim.recovery.update_state, UpdateState::Idle);
    assert!(sim.recovery.staged_packages.is_empty());
    // Repair is labelled and only clears simulated damage.
    sim.recovery.interrupted_dpkg = true;
    let Reply::Message(repaired) = sim.request(Request::RecoverUpdates).unwrap() else {
        panic!("simulated repair");
    };
    assert!(repaired.starts_with("SIMULATED"));
    assert!(!sim.recovery.interrupted_dpkg);
    // Still-refused requests stay refused.
    assert!(
        sim.request(Request::Power(crate::PowerAction::Suspend))
            .is_err()
    );
}
