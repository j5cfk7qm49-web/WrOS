use super::*;
use crate::PowerAction;
use std::time::{Duration, Instant};
#[derive(Default)]
struct Fake(Vec<Invocation>);
impl Runner for Fake {
    fn run(&mut self, c: &Invocation) -> Result<String> {
        self.0.push(c.clone());
        Ok("disconnected:none".into())
    }
}
#[test]
fn unfinished_mutations_cannot_invoke_tools_or_fabricate_service_success() {
    let mut backend = Linux {
        runner: Fake::default(),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    for request in [
        Request::ForgetWifi("ambiguous profile".into()),
        Request::ActivateSavedWifi("not-a-uuid".into()),
        Request::ConnectWifi {
            ssid: "--invalid-flag".into(),
            password: None,
        },
        Request::ConnectWifi {
            ssid: "ValidSSID".into(),
            password: Some(crate::WifiSecret::new("short")),
        },
        Request::VerifyUpdateBundle {
            bundle_dir: "relative-bundle".into(),
        },
        Request::VerifyUpdateBundle {
            bundle_dir: "/media/../etc".into(),
        },
        Request::ConnectGoogleAccount,
        Request::DisconnectGoogleAccount,
        Request::ListGoogleDriveFiles,
        Request::MaterializeDriveFile {
            file_id: "file-01".into(),
            local_name: "draft.md".into(),
        },
        Request::ListBackups,
        Request::CreateBackup {
            target_dir: "/unused".into(),
        },
        Request::RestoreBackup {
            snapshot_id: "latest".into(),
            restore_dir: "/unused".into(),
        },
        Request::VerifyBackups,
        Request::StageUpdates {
            bundle_dir: "/media/usb/wros-updates".into(),
            review_id: "not-a-review-id".into(),
        },
        Request::StageUpdates {
            bundle_dir: "/etc".into(),
            review_id: "a".repeat(64),
        },
        Request::ReviewUpdateBundle {
            bundle_dir: "/var/lib/wros-update".into(),
        },
        Request::RebootRecovery,
        Request::RebootFirmware,
    ] {
        assert!(backend.request(request).is_err());
    }
    assert!(backend.runner.0.is_empty());
    // Even with valid arguments, unverified Fake output ("disconnected:none")
    // is refused by the readback/JSON verification checks.
    assert!(
        backend
            .request(Request::ConnectWifi {
                ssid: "ValidSSID".into(),
                password: None,
            })
            .is_err()
    );
    assert!(backend.request(Request::DisconnectWifi).is_err());
    assert!(backend.request(Request::RecoveryStatus).is_err());
    assert!(
        backend
            .request(Request::VerifyUpdateBundle {
                bundle_dir: "/media/usb/wros-updates".into()
            })
            .is_err()
    );
    assert!(backend.request(Request::CancelStagedUpdates).is_err());
    assert!(backend.request(Request::RecoverUpdates).is_err());
    let Reply::GoogleStatus(status) = backend.request(Request::GetGoogleStatus).unwrap() else {
        panic!()
    };
    assert!(!status.connected);
    assert!(status.email.is_none());
    assert!(
        backend.request(Request::ListLocalAccounts).is_err(),
        "missing passwd is an error, never a fabricated writer account"
    );
}
#[test]
fn import_media_verbs_use_pkexec_and_the_media_budget() {
    for (request, args) in [
        (Request::ListImportMedia, vec!["list"]),
        (
            Request::MountImportMedia("/dev/sdb1".into()),
            vec!["mount", "/dev/sdb1"],
        ),
        (Request::UnmountImportMedia, vec!["unmount"]),
    ] {
        let command = invocation(&request).unwrap();
        assert_eq!(command.program, crate::paths::PKEXEC);
        let mut expected = vec!["--disable-internal-agent", crate::paths::WROS_IMPORT_MEDIA];
        expected.extend(args);
        assert_eq!(command.args, expected);
        assert_eq!(command.timeout, Duration::from_secs(60));
    }
}

#[test]
fn import_media_replies_decode_the_stick_and_require_mount_confirmation() {
    struct MediaReply(&'static str);
    impl Runner for MediaReply {
        fn run(&mut self, _: &Invocation) -> Result<String> {
            Ok(self.0.into())
        }
    }
    let backend = |output| Linux {
        runner: MediaReply(output),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let listed = r#"[{"device":"/dev/sdb1","label":"BOOKS","mount":null}]"#;
    assert_eq!(
        backend(listed).request(Request::ListImportMedia).unwrap(),
        Reply::ImportMedia(vec![crate::ImportStick {
            device: "/dev/sdb1".into(),
            label: "BOOKS".into(),
            mount: None,
        }])
    );
    let mounted = concat!(
        r#"[{"device":"/dev/sdb1","label":"BOOKS","mount":"#,
        r#""/run/wros-import/media/1"}]"#,
    );
    let mount = Request::MountImportMedia("/dev/sdb1".into());
    assert!(matches!(
        backend(mounted).request(mount.clone()).unwrap(),
        Reply::ImportMedia(sticks) if sticks[0].mount.is_some()
    ));
    assert_eq!(
        backend("[]").request(Request::UnmountImportMedia).unwrap(),
        Reply::ImportMedia(Vec::new())
    );
    for output in ["not json", "{}", "[]", listed] {
        assert!(backend(output).request(mount.clone()).is_err());
    }
    assert!(
        backend(mounted)
            .request(Request::MountImportMedia("/dev/sdc1".into()))
            .is_err()
    );
    assert!(
        backend(mounted)
            .request(Request::UnmountImportMedia)
            .is_err()
    );
}

#[test]
fn update_verbs_use_fixed_argv_and_only_privileged_verbs_use_pkexec() {
    let dir = "/media/usb/wros-updates";
    let id = "0123456789abcdef".repeat(4);
    let verify = invocation(&Request::VerifyUpdateBundle {
        bundle_dir: dir.into(),
    })
    .unwrap();
    assert_eq!(verify.program, crate::paths::WROS_UPDATE);
    assert_eq!(verify.args, ["verify-bundle", dir]);

    // Review is unprivileged and never goes through pkexec.
    let review = invocation(&Request::ReviewUpdateBundle {
        bundle_dir: dir.into(),
    })
    .unwrap();
    assert_eq!(review.program, crate::paths::WROS_UPDATE);
    assert_eq!(review.args, ["review", dir]);
    assert_eq!(review.timeout, Duration::from_secs(180));

    let pk = "--disable-internal-agent";
    let stage = invocation(&Request::StageUpdates {
        bundle_dir: dir.into(),
        review_id: id.clone(),
    })
    .unwrap();
    assert_eq!(stage.program, crate::paths::PKEXEC);
    assert_eq!(
        stage.args,
        [pk, crate::paths::WROS_UPDATE_STAGE, dir, id.as_str()]
    );
    assert_eq!(stage.timeout, Duration::from_secs(900));

    for (request, helper, args, seconds) in [
        (
            Request::CancelStagedUpdates,
            crate::paths::WROS_UPDATE_CANCEL,
            vec![],
            60,
        ),
        (
            Request::RecoverUpdates,
            crate::paths::WROS_UPDATE_REPAIR,
            vec![],
            900,
        ),
        (
            Request::FindUpdateMedia,
            crate::paths::WROS_UPDATE_MEDIA,
            vec!["mount"],
            60,
        ),
    ] {
        let command = invocation(&request).unwrap();
        assert_eq!(command.program, crate::paths::PKEXEC);
        let mut expected = vec![pk, helper];
        expected.extend(args);
        assert_eq!(command.args, expected);
        assert_eq!(command.timeout, Duration::from_secs(seconds));
    }
    // Read-only status and display calls keep their existing budgets.
    assert_eq!(
        invocation(&Request::RecoveryStatus).unwrap().timeout,
        Duration::from_secs(60)
    );
    assert_eq!(
        invocation(&Request::DisplayStatus).unwrap().timeout,
        Duration::from_secs(8)
    );
}
#[test]
fn update_requests_with_bad_paths_or_review_ids_never_reach_the_runner() {
    let mut backend = Linux {
        runner: Fake::default(),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let good_id = "ab".repeat(32);
    for bad_dir in [
        "relative/bundle",
        "/",
        "/media/../etc",
        "/etc",
        "/var/lib/wros-update/public",
        "/run/wros-update",
        "/run/wros-update/staging",
        "/home/writer/novel",
        "/media",
        "/media/",
        "/media/usb/",
        "/media//usb",
        "/media/./usb",
        "/mnt",
        "/proc/self",
        "/media/usb\nreview",
    ] {
        for request in [
            Request::ReviewUpdateBundle {
                bundle_dir: bad_dir.into(),
            },
            Request::StageUpdates {
                bundle_dir: bad_dir.into(),
                review_id: good_id.clone(),
            },
        ] {
            assert!(backend.request(request).is_err(), "{bad_dir:?}");
        }
    }
    let bad_ids: Vec<String> = vec![
        String::new(),
        "short".into(),
        "A".repeat(64),
        "g".repeat(64),
        "a".repeat(63),
        "a".repeat(65),
        format!("{}\n", "a".repeat(63)),
        "--help".into(),
    ];
    for bad_id in bad_ids {
        assert!(
            backend
                .request(Request::StageUpdates {
                    bundle_dir: "/media/usb/wros-updates".into(),
                    review_id: bad_id.clone(),
                })
                .is_err(),
            "{bad_id:?}"
        );
    }
    assert!(backend.runner.0.is_empty());
    for ok in [
        "/media/usb/wros-updates",
        "/run/media/writer/WROS-UPDATE",
        "/mnt/update",
        "/run/wros-update/media/0",
    ] {
        assert!(
            crate::system_views::validate_update_bundle_path(ok).is_ok(),
            "{ok}"
        );
    }
}
#[test]
fn fabricated_update_output_is_never_success() {
    // Fake answers every command with the unrelated text "disconnected:none".
    let mut backend = Linux {
        runner: Fake::default(),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    for request in [
        Request::CancelStagedUpdates,
        Request::RecoverUpdates,
        Request::FindUpdateMedia,
        Request::ReviewUpdateBundle {
            bundle_dir: "/media/usb/wros-updates".into(),
        },
        Request::StageUpdates {
            bundle_dir: "/media/usb/wros-updates".into(),
            review_id: "ab".repeat(32),
        },
    ] {
        assert!(backend.request(request).is_err());
    }
    assert_eq!(backend.runner.0.len(), 5);
}
#[test]
fn a_password_change_sends_both_secrets_on_stdin_and_reports_the_helpers_answer() {
    // #121: /usr/libexec/wros-passwd drives passwd; secrets never reach argv or errors.
    struct PasswdRunner {
        reply: &'static str,
        calls: Vec<(Invocation, String)>,
    }
    impl Runner for PasswdRunner {
        fn run(&mut self, cmd: &Invocation) -> Result<String> {
            self.run_with_input(cmd, &[])
        }
        fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
            self.calls
                .push((cmd.clone(), String::from_utf8_lossy(input).into_owned()));
            Ok(self.reply.into())
        }
    }
    let backend = |reply| Linux {
        runner: PasswdRunner {
            reply,
            calls: Vec::new(),
        },
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let change = |current: &str, new: &str| Request::ChangePassword {
        current: Some(crate::WifiSecret::new(current)),
        new: Some(crate::WifiSecret::new(new)),
    };
    let mut ok = backend(
        r#"{"changed": true, "message": "Password changed. Use it to sign in and to unlock."}"#,
    );
    let reply = ok
        .request(change("OLD_SECRET_VALUE", "NEW_SECRET_VALUE"))
        .unwrap();
    assert_eq!(
        reply,
        Reply::Message("Password changed. Use it to sign in and to unlock.".into())
    );
    let (cmd, input) = &ok.runner.calls[0];
    assert_eq!(cmd.program, crate::paths::WROS_PASSWD);
    assert!(cmd.args.is_empty());
    assert_eq!(input, "OLD_SECRET_VALUE\nNEW_SECRET_VALUE\n");
    assert!(!format!("{:?}", change("OLD_SECRET_VALUE", "x")).contains("OLD_SECRET_VALUE"));

    let mut wrong = backend(r#"{"changed": false, "message": "The current password is wrong."}"#);
    let error = wrong
        .request(change("OLD_SECRET_VALUE", "NEW_SECRET_VALUE"))
        .unwrap_err()
        .to_string();
    assert_eq!(error, "The current password is wrong.");

    for bad in [
        "",
        "not json",
        r#"{"changed": true}"#,
        r#"{"changed": true, "message": "", "x": 1}"#,
    ] {
        let mut odd = backend(bad);
        assert!(odd.request(change("a", "b")).is_err(), "{bad}");
    }
    // Refused before the helper runs: missing, multi-line or oversized secrets.
    let mut never = backend("{}");
    for request in [
        Request::ChangePassword {
            current: None,
            new: Some(crate::WifiSecret::new("b")),
        },
        change("a\nb", "c"),
        change("a", &"x".repeat(600)),
    ] {
        assert!(never.request(request).is_err());
    }
    assert!(never.runner.calls.is_empty());
}

#[test]
fn wifi_enrollment_passes_secret_on_stdin_never_argv_and_verifies_active_connection() {
    use std::collections::VecDeque;
    struct WifiRunner {
        replies: VecDeque<Result<&'static str, &'static str>>,
        calls: Vec<(Invocation, String)>,
    }
    impl Runner for WifiRunner {
        fn run(&mut self, cmd: &Invocation) -> Result<String> {
            self.run_with_input(cmd, &[])
        }
        fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
            self.calls
                .push((cmd.clone(), String::from_utf8_lossy(input).into_owned()));
            match self.replies.pop_front().unwrap() {
                Ok(out) => Ok(out.into()),
                Err(err) => bail!("{err}"),
            }
        }
    }
    let uuid = "00000000-0000-4000-8000-000000000001";
    let active_line = "00000000-0000-4000-8000-000000000001:WriterDeck-Cafe:802-11-wireless";
    let mut backend = Linux {
        runner: WifiRunner {
            replies: VecDeque::from([
                Ok("Device 'wlan0' successfully activated."),
                Ok("*:WriterDeck-Cafe:88:WPA2"),
                Ok("Connection 'WriterDeck-Cafe' successfully activated."),
                Ok(active_line),
                Ok(active_line),
                Ok("Connection 'WriterDeck-Cafe' successfully deactivated."),
                Ok(
                    "Connection 'WriterDeck-Cafe' (00000000-0000-4000-8000-000000000001) successfully deleted.",
                ),
            ]),
            calls: Vec::new(),
        },
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let secret = crate::WifiSecret::new("SUPER_SECRET_PSK_VALUE");
    let reply = backend
        .request(Request::ConnectWifi {
            ssid: "WriterDeck-Cafe".into(),
            password: Some(secret),
        })
        .unwrap();
    assert_eq!(
        reply,
        Reply::Message("Connected to Wi-Fi network WriterDeck-Cafe.".into())
    );
    assert_eq!(backend.runner.calls[0].0.program, crate::paths::NMCLI);
    assert!(
        backend.runner.calls[0]
            .0
            .args
            .iter()
            .all(|a| !a.contains("SUPER_SECRET_PSK_VALUE"))
    );
    assert!(
        backend.runner.calls[0]
            .0
            .args
            .contains(&"--ask".to_string())
    );
    assert_eq!(backend.runner.calls[0].1, "SUPER_SECRET_PSK_VALUE\n");

    // Reconnect saved Wi-Fi by exact UUID and verify active state.
    assert_eq!(
        backend
            .request(Request::ActivateSavedWifi(uuid.into()))
            .unwrap(),
        Reply::Message(format!("Connected saved Wi-Fi profile {uuid}."))
    );
    // Disconnect active Wi-Fi without assuming an interface name.
    assert_eq!(
        backend.request(Request::DisconnectWifi).unwrap(),
        Reply::Message("Wi-Fi disconnected.".into())
    );
    // Forget saved Wi-Fi profile by UUID.
    assert_eq!(
        backend.request(Request::ForgetWifi(uuid.into())).unwrap(),
        Reply::Message(format!("Removed saved Wi-Fi profile {uuid}."))
    );

    let mut unrelated = Linux {
        runner: WifiRunner {
            replies: VecDeque::from([Ok("success"), Ok("*:OtherSSID:90:WPA2")]),
            calls: Vec::new(),
        },
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    assert!(
        unrelated
            .request(Request::ConnectWifi {
                ssid: "WriterDeck-Cafe".into(),
                password: None
            })
            .is_err()
    );

    // Failed authentication redacts any echoed secret from the error.
    let mut failing = Linux {
        runner: WifiRunner {
            replies: VecDeque::from([Err(
                "Secrets were required, rejected SUPER_SECRET_PSK_VALUE",
            )]),
            calls: Vec::new(),
        },
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let err = failing
        .request(Request::ConnectWifi {
            ssid: "WriterDeck-Cafe".into(),
            password: Some(crate::WifiSecret::new("SUPER_SECRET_PSK_VALUE")),
        })
        .unwrap_err()
        .to_string();
    assert!(!err.contains("SUPER_SECRET_PSK_VALUE"));
    assert!(err.contains("[hidden]"));
}
#[test]
fn read_only_views_use_structured_fixed_commands_without_secrets_or_assumed_interfaces() {
    let scan = invocation(&Request::ScanWifi).unwrap();
    assert_eq!(scan.program, crate::paths::NMCLI);
    assert!(scan.args.windows(2).any(|p| p == ["--escape", "yes"]));
    let profiles = invocation(&Request::ListSavedWifi).unwrap();
    assert!(profiles.args.contains(&"UUID,NAME,TYPE".into()));
    let disks = invocation(&Request::ListStorageDevices).unwrap();
    assert_eq!(disks.program, crate::paths::FINDMNT);
    assert!(disks.args.contains(&"--json".into()));
    assert_eq!(
        invocation(&Request::CheckUpdates).unwrap().args,
        ["-s", "dist-upgrade"]
    );
}
#[test]
fn process_timeout_also_bounds_pipes_inherited_by_a_child() {
    let mut command = Invocation::new(crate::paths::SH, &["-c", "sleep 2 & exit 0"]);
    command.timeout = Duration::from_millis(100);
    let started = Instant::now();
    let result = ProcessRunner.run(&command);
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn process_runner_rejects_truncation_and_invalid_utf8_instead_of_partial_success() {
    let mut runner = ProcessRunner;
    assert!(
        runner
            .run(&Invocation::new(
                crate::paths::PYTHON3,
                &["-c", "import sys; sys.stdout.write('x'*65537)"]
            ))
            .is_err()
    );
    assert!(
        runner
            .run(&Invocation::new(
                crate::paths::PYTHON3,
                &["-c", "import sys; sys.stdout.buffer.write(bytes([255]))"]
            ))
            .is_err()
    );
    assert_eq!(
        runner
            .run(&Invocation::new(
                crate::paths::PYTHON3,
                &["-c", "print('  name  ')"]
            ))
            .unwrap(),
        "  name  "
    );
    assert_eq!(
        runner
            .run_with_input(
                &Invocation::new(
                    crate::paths::PYTHON3,
                    &["-c", "import sys; print(sys.stdin.readline().strip())"]
                ),
                b"stdin-secret-line\n"
            )
            .unwrap(),
        "stdin-secret-line"
    );
}
#[test]
fn actual_debian_findmnt_json_matches_the_storage_view_contract() {
    let mut runner = ProcessRunner;
    let output = runner
        .run(&Invocation::new(
            crate::paths::FINDMNT,
            &[
                "--json",
                "--list",
                "--bytes",
                "--df",
                "--target",
                ".",
                "--output",
                "SOURCE,TARGET,SIZE,USED,AVAIL,USE%",
            ],
        ))
        .unwrap();
    let devices = crate::system_views::storage(&output).unwrap();
    assert_eq!(devices.len(), 1);
    assert!(
        std::env::current_dir()
            .unwrap()
            .starts_with(&devices[0].mountpoint)
    );
    assert!(devices[0].total_bytes > 0);
}
#[test]
fn arguments_cannot_be_shell_commands() {
    assert!(invocation(&Request::KeyboardConfirm(";reboot".into())).is_err());
    assert!(invocation(&Request::Brightness(0)).is_err());
    let c = invocation(&Request::Power(PowerAction::Shutdown)).unwrap();
    assert_eq!(c.program, crate::paths::SYSTEMCTL);
    assert_eq!(c.args, ["--no-ask-password", "poweroff"]);
    assert!(
        invocation(&Request::Wifi(false))
            .unwrap()
            .args
            .contains(&"off".to_string())
    );
}
#[test]
fn missing_hardware_is_unavailable_not_a_fake_battery() {
    let mut b = Linux {
        runner: Fake::default(),
        sys: "/does-not-exist".into(),
        etc: "/does-not-exist".into(),
        storage: "/does-not-exist".into(),
    };
    let s = b.snapshot();
    assert_eq!(s.battery_percent, None);
    assert_eq!(s.brightness_percent, None);
    assert!(!s.simulated);
    assert_eq!(s.available_bytes, None);
}
#[test]
fn real_command_errors_are_not_reported_as_success() {
    let r = ProcessRunner.run(&Invocation::new(crate::paths::USR_BIN_FALSE, &[]));
    assert!(r.is_err());
}
#[test]
fn unsupported_suspend_never_reaches_a_process_runner() {
    let mut backend = Linux {
        runner: Fake::default(),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    let error = backend
        .request(Request::Power(PowerAction::Suspend))
        .unwrap_err();
    assert!(error.to_string().contains("secure screen locking"));
    assert!(
        backend.runner.0.is_empty(),
        "not even a systemctl probe may run"
    );
    assert_eq!(
        invocation(&Request::Power(PowerAction::Restart))
            .unwrap()
            .args,
        ["--no-ask-password", "reboot"]
    );
    assert_eq!(
        invocation(&Request::Power(PowerAction::Shutdown))
            .unwrap()
            .args,
        ["--no-ask-password", "poweroff"]
    );
    assert!(
        crate::Simulated::laptop()
            .request(Request::Power(PowerAction::Suspend))
            .is_err()
    );
}
#[test]
fn timezone_change_is_fixed_noninteractive_and_requires_matching_readback() {
    use std::collections::VecDeque;
    struct Replies(VecDeque<&'static str>, Vec<Invocation>);
    impl Runner for Replies {
        fn run(&mut self, invocation: &Invocation) -> Result<String> {
            self.1.push(invocation.clone());
            let output = self.0.pop_front().unwrap();
            if output == "FAIL" {
                bail!("fixture service unavailable");
            }
            Ok(output.into())
        }
    }
    for (outputs, accepted) in [
        (vec!["", "Asia/Tokyo"], true),
        (vec!["", "UTC"], false),
        (vec!["", "bad output"], false),
        (vec!["", "FAIL"], false),
        (vec!["FAIL"], false),
    ] {
        let mut backend = Linux {
            runner: Replies(outputs.into(), vec![]),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        let result = backend.request(Request::SetTimezone("Asia/Tokyo".into()));
        assert_eq!(result.is_ok(), accepted);
        if accepted {
            assert_eq!(result.unwrap(), Reply::Timezone("Asia/Tokyo".into()));
        }
        assert_eq!(
            backend.runner.1[0],
            Invocation::new(
                crate::paths::TIMEDATECTL,
                &[
                    "--no-ask-password",
                    "--no-pager",
                    "set-timezone",
                    "Asia/Tokyo"
                ]
            )
        );
    }
    let mut backend = Linux {
        runner: Fake::default(),
        sys: "/missing".into(),
        etc: "/missing".into(),
        storage: "/missing".into(),
    };
    assert!(
        backend
            .request(Request::SetTimezone("--host=other".into()))
            .is_err()
    );
    assert!(backend.runner.0.is_empty());
    assert!(
        backend.request(Request::ListTimezones).is_err(),
        "invalid service output must not become a catalog"
    );
}
#[test]
fn start_screen_calls_are_noninteractive_and_require_a_confirmed_choice() {
    let theme = "name = \"Nord\"\n";
    let request = Request::BootScreen {
        enabled: true,
        theme: theme.into(),
    };
    let command = invocation(&request).unwrap();
    assert_eq!(command.program, crate::paths::PKEXEC);
    assert_eq!(
        command.args,
        [
            "--disable-internal-agent",
            crate::paths::WROS_BOOT_SETTINGS,
            "screen",
            "ratatui",
            theme
        ]
    );
    assert_eq!(command.timeout, Duration::from_secs(8));
    for bad in [String::new(), "x".repeat(4097), "name = \"a\0b\"".into()] {
        assert!(invocation(&Request::BootTheme(bad)).is_err());
    }
    struct Output(&'static str);
    impl Runner for Output {
        fn run(&mut self, _: &Invocation) -> Result<String> {
            Ok(self.0.into())
        }
    }
    for (output, accepted) in [
        (r#"{"enabled":true,"pending":true,"failed":false}"#, true),
        (r#"{"enabled":false,"pending":true,"failed":false}"#, false),
        (r#"{"enabled":true}"#, false),
        (
            r#"{"enabled":true,"pending":false,"failed":false,"unexpected":true}"#,
            false,
        ),
        ("not JSON", false),
    ] {
        let mut backend = Linux {
            runner: Output(output),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        assert_eq!(
            backend.request(request.clone()).is_ok(),
            accepted,
            "{output}"
        );
    }
}

#[test]
fn display_calls_are_fixed_bounded_and_never_start_a_tty_auth_reader() {
    use crate::display::Warmth;
    let command = invocation(&Request::DisplayPreview(Warmth::default())).unwrap();
    assert_eq!(command.program, crate::paths::PKEXEC);
    assert_eq!(
        command.args,
        [
            "--disable-internal-agent",
            crate::paths::WROS_DISPLAY,
            "preview",
            "on",
            "2200",
            "100"
        ]
    );
    assert!(
        invocation(&Request::DisplayPreview(Warmth {
            kelvin: 999,
            ..Warmth::default()
        }))
        .is_err()
    );
    assert!(invocation(&Request::DisplayConfirm(";rm -rf /".into())).is_err());
    assert_eq!(
        invocation(&Request::KeyboardPreview(
            crate::keyboard::Layout::ALL[0].clone()
        ))
        .unwrap()
        .args[0],
        "--disable-internal-agent"
    );
}
#[test]
fn malformed_or_expired_display_replies_never_become_successful_previews() {
    struct Output(&'static str);
    impl Runner for Output {
        fn run(&mut self, _: &Invocation) -> Result<String> {
            Ok(self.0.into())
        }
    }
    for output in [
        r#"{"token":"bad","seconds":15}"#,
        r#"{"token":"0123456789abcdef0123456789abcdef","seconds":0}"#,
        r#"{"token":"0123456789abcdef0123456789abcdef","seconds":16}"#,
    ] {
        let mut backend = Linux {
            runner: Output(output),
            sys: "/missing".into(),
            etc: "/missing".into(),
            storage: "/missing".into(),
        };
        assert!(
            backend
                .request(Request::DisplayPreview(crate::display::Warmth::default()))
                .is_err()
        );
    }
}
