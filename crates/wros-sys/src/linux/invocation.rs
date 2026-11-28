//! Fixed executable paths, argument vectors and timeouts for requests.
use crate::{PowerAction, Request};
use anyhow::{Result, bail};
use std::time::Duration;

/// Longest the runner waits for each privileged update verb. Staging verifies
/// and copies a whole bundle, so it is far longer than a display preview.
pub const REVIEW_TIMEOUT_SECS: u64 = 180;
pub const STAGE_TIMEOUT_SECS: u64 = 900;
pub const REPAIR_TIMEOUT_SECS: u64 = 900;
pub const CANCEL_TIMEOUT_SECS: u64 = 60;
pub const APPLY_TIMEOUT_SECS: u64 = 900;
pub const MEDIA_TIMEOUT_SECS: u64 = 60;
pub const ONLINE_CHECK_TIMEOUT_SECS: u64 = 30;
/// Downloading a whole update over a slow connection.
pub const ONLINE_FETCH_TIMEOUT_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<String>,
    pub timeout: Duration,
}
impl Invocation {
    pub(super) fn new(program: &'static str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
            timeout: Duration::from_secs(if program == crate::paths::WROS_UPDATE {
                60
            } else if program == crate::paths::NMCLI {
                20
            } else {
                8
            }),
        }
    }
    #[must_use]
    fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout = Duration::from_secs(seconds);
        self
    }
    /// Fixed argv for a narrow privileged WrOS update launcher. pkexec never
    /// starts its own text agent, so a missing polkit rule fails instead of
    /// prompting on the writer's terminal.
    fn update_launcher(helper: &'static str, args: &[&str], timeout_secs: u64) -> Self {
        let mut argv = vec!["--disable-internal-agent", helper];
        argv.extend_from_slice(args);
        Self::new(crate::paths::PKEXEC, &argv).with_timeout(timeout_secs)
    }
}

pub(super) fn timezone_status() -> Invocation {
    Invocation::new(
        crate::paths::TIMEDATECTL,
        &[
            "--no-ask-password",
            "--no-pager",
            "--value",
            "--property=Timezone",
            "show",
        ],
    )
}
pub(super) fn active_wifi_connections() -> Invocation {
    Invocation::new(
        crate::paths::NMCLI,
        &[
            "--colors",
            "no",
            "-t",
            "--escape",
            "yes",
            "-f",
            "UUID,NAME,TYPE",
            "connection",
            "show",
            "--active",
        ],
    )
}

pub fn invocation(request: &Request) -> Result<Invocation> {
    Ok(match request {
        Request::Refresh => bail!("refresh is a composite read-only operation"),
        Request::ListTimezones => Invocation::new(
            crate::paths::TIMEDATECTL,
            &["--no-ask-password", "--no-pager", "list-timezones"],
        ),
        Request::SetTimezone(zone) => {
            crate::timezone::validate(zone)?;
            Invocation::new(
                crate::paths::TIMEDATECTL,
                &["--no-ask-password", "--no-pager", "set-timezone", zone],
            )
        }
        Request::SetLocalClock(ts) => {
            let normalized = crate::timezone::validate_local_datetime(ts)?;
            Invocation::new(
                crate::paths::PKEXEC,
                &[
                    "--disable-internal-agent",
                    crate::paths::WROS_CLOCK,
                    "set-time",
                    &normalized,
                ],
            )
        }
        Request::UseNetworkTime => Invocation::new(
            crate::paths::PKEXEC,
            &[
                "--disable-internal-agent",
                crate::paths::WROS_CLOCK,
                "network-time",
            ],
        ),
        Request::AudioStatus => Invocation::new(crate::paths::WROS_SOUND, &["status"]),
        Request::SetVolume(vol) => {
            if *vol > 100 {
                bail!("volume must be 0..=100");
            }
            Invocation::new(crate::paths::WROS_SOUND, &["set", &vol.to_string()])
        }
        Request::SetMute(muted) => Invocation::new(
            crate::paths::WROS_SOUND,
            &[if *muted { "mute" } else { "unmute" }],
        ),
        Request::Wifi(on) => Invocation::new(
            crate::paths::NMCLI,
            &["radio", "wifi", if *on { "on" } else { "off" }],
        ),
        Request::ScanWifi => Invocation::new(
            crate::paths::NMCLI,
            &[
                "--colors",
                "no",
                "-t",
                "--escape",
                "yes",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "yes",
            ],
        ),
        Request::ConnectWifi { ssid, password } => {
            crate::system_views::validate_wifi_ssid(ssid)?;
            if let Some(secret) = password {
                crate::system_views::validate_wifi_password(secret.as_str())?;
                Invocation::new(
                    crate::paths::NMCLI,
                    &[
                        "--colors", "no", "--ask", "--wait", "15", "device", "wifi", "connect",
                        ssid,
                    ],
                )
            } else {
                Invocation::new(
                    crate::paths::NMCLI,
                    &[
                        "--colors", "no", "--wait", "8", "device", "wifi", "connect", ssid,
                    ],
                )
            }
        }
        Request::ActivateSavedWifi(uuid) => {
            crate::system_views::validate_saved_wifi_uuid(uuid)?;
            Invocation::new(
                crate::paths::NMCLI,
                &[
                    "--colors",
                    "no",
                    "--wait",
                    "8",
                    "connection",
                    "up",
                    "uuid",
                    uuid,
                ],
            )
        }
        Request::DisconnectWifi => active_wifi_connections(),
        Request::ListSavedWifi => Invocation::new(
            crate::paths::NMCLI,
            &[
                "--colors",
                "no",
                "-t",
                "--escape",
                "yes",
                "-f",
                "UUID,NAME,TYPE",
                "connection",
                "show",
            ],
        ),
        Request::ForgetWifi(uuid) => {
            crate::system_views::validate_saved_wifi_uuid(uuid)?;
            Invocation::new(
                crate::paths::NMCLI,
                &["--colors", "no", "connection", "delete", "uuid", uuid],
            )
        }
        Request::CheckUpdates => Invocation::new(crate::paths::APT_GET, &["-s", "dist-upgrade"]),
        Request::RecoveryStatus => Invocation::new(crate::paths::WROS_UPDATE, &["recovery-status"]),
        Request::VerifyUpdateBundle { bundle_dir } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            Invocation::new(crate::paths::WROS_UPDATE, &["verify-bundle", bundle_dir])
        }
        // Unprivileged: authenticates the bundle and lists it; changes nothing.
        Request::ReviewUpdateBundle { bundle_dir } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            Invocation::new(crate::paths::WROS_UPDATE, &["review", bundle_dir])
                .with_timeout(REVIEW_TIMEOUT_SECS)
        }
        Request::StageUpdates {
            bundle_dir,
            review_id,
        } => {
            crate::system_views::validate_update_bundle_path(bundle_dir)?;
            crate::system_views::validate_review_id(review_id)?;
            Invocation::update_launcher(
                crate::paths::WROS_UPDATE_STAGE,
                &[bundle_dir, review_id],
                STAGE_TIMEOUT_SECS,
            )
        }
        Request::FindUpdateMedia => Invocation::update_launcher(
            crate::paths::WROS_UPDATE_MEDIA,
            &["mount"],
            MEDIA_TIMEOUT_SECS,
        ),
        Request::ListImportMedia => Invocation::update_launcher(
            crate::paths::WROS_IMPORT_MEDIA,
            &["list"],
            MEDIA_TIMEOUT_SECS,
        ),
        Request::MountImportMedia(device) => Invocation::update_launcher(
            crate::paths::WROS_IMPORT_MEDIA,
            &["mount", device],
            MEDIA_TIMEOUT_SECS,
        ),
        Request::UnmountImportMedia => Invocation::update_launcher(
            crate::paths::WROS_IMPORT_MEDIA,
            &["unmount"],
            MEDIA_TIMEOUT_SECS,
        ),
        // Unprivileged: signed metadata only, from the fixed URL in the engine.
        Request::CheckOnlineUpdates => {
            Invocation::new(crate::paths::WROS_UPDATE, &["check-online"])
                .with_timeout(ONLINE_CHECK_TIMEOUT_SECS)
        }
        Request::FetchOnlineUpdate => Invocation::update_launcher(
            crate::paths::WROS_UPDATE_FETCH,
            &[],
            ONLINE_FETCH_TIMEOUT_SECS,
        ),
        Request::AddDeviceFirmware => Invocation::update_launcher(
            crate::paths::WROS_UPDATE_FIRMWARE,
            &[],
            ONLINE_FETCH_TIMEOUT_SECS,
        ),
        Request::CancelStagedUpdates => {
            Invocation::update_launcher(crate::paths::WROS_UPDATE_CANCEL, &[], CANCEL_TIMEOUT_SECS)
        }
        Request::ApplyStagedUpdate => {
            Invocation::update_launcher(crate::paths::WROS_UPDATE_APPLY, &[], APPLY_TIMEOUT_SECS)
        }
        Request::GoBackToPreviousVersion => {
            Invocation::update_launcher(crate::paths::WROS_UPDATE_ROLLBACK, &[], STAGE_TIMEOUT_SECS)
        }
        Request::ReportUpdateReady => Invocation::update_launcher(
            crate::paths::WROS_UPDATE_HEALTHY,
            &["ready"],
            CANCEL_TIMEOUT_SECS,
        ),
        Request::RecoverUpdates => {
            Invocation::update_launcher(crate::paths::WROS_UPDATE_REPAIR, &[], REPAIR_TIMEOUT_SECS)
        }
        Request::ListStorageDevices => Invocation::new(
            crate::paths::FINDMNT,
            &[
                "--json",
                "--list",
                "--bytes",
                "--df",
                "--output",
                "SOURCE,TARGET,SIZE,USED,AVAIL,USE%",
            ],
        ),
        Request::ConnectGoogleAccount
        | Request::DisconnectGoogleAccount
        | Request::ListGoogleDriveFiles
        | Request::MaterializeDriveFile { .. }
        | Request::ListBackups
        | Request::CreateBackup { .. }
        | Request::RestoreBackup { .. }
        | Request::VerifyBackups
        | Request::RebootRecovery
        | Request::RebootFirmware => {
            bail!("this native workflow is not implemented; no action was performed")
        }
        Request::ListLocalAccounts | Request::GetGoogleStatus => {
            bail!("read-only local request handled directly by Linux backend")
        }
        Request::ChangePassword { .. } => Invocation::new(crate::paths::WROS_PASSWD, &[]),
        Request::Brightness(p) => {
            if !(1..=100).contains(p) {
                bail!("brightness must be 1..100");
            }
            Invocation::new(
                crate::paths::BRIGHTNESSCTL,
                &["--class=backlight", "set", &format!("{p}%")],
            )
        }
        Request::Power(action) => Invocation::new(
            crate::paths::SYSTEMCTL,
            &[
                "--no-ask-password",
                match action {
                    PowerAction::Suspend => bail!(crate::SUSPEND_UNAVAILABLE),
                    PowerAction::Shutdown => "poweroff",
                    PowerAction::Restart => "reboot",
                },
            ],
        ),
        Request::PowerSave(on) => Invocation::new(
            crate::paths::POWERPROFILESCTL,
            &["set", if *on { "power-saver" } else { "balanced" }],
        ),
        Request::KeyboardPreview(l) => Invocation::new(
            crate::paths::PKEXEC,
            &[
                "--disable-internal-agent",
                crate::paths::WROS_KEYBOARD,
                "preview",
                l.id(),
            ],
        ),
        Request::KeyboardConfirm(t) | Request::KeyboardCancel(t) => {
            if t.len() != 32 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("invalid keyboard preview token");
            }
            Invocation::new(
                crate::paths::PKEXEC,
                &[
                    "--disable-internal-agent",
                    crate::paths::WROS_KEYBOARD,
                    if matches!(request, Request::KeyboardConfirm(_)) {
                        "confirm"
                    } else {
                        "cancel"
                    },
                    t,
                ],
            )
        }
        Request::BootStatus | Request::BootTheme(_) | Request::BootScreen { .. } => {
            let mut args = vec!["--disable-internal-agent", crate::paths::WROS_BOOT_SETTINGS];
            match request {
                Request::BootStatus => args.push("status"),
                Request::BootTheme(theme) | Request::BootScreen { theme, .. } => {
                    if theme.is_empty() || theme.len() > 4096 || theme.contains('\0') {
                        bail!("invalid start-screen theme");
                    }
                    if let Request::BootScreen { enabled, .. } = request {
                        args.extend(["screen", if *enabled { "ratatui" } else { "text" }]);
                    } else {
                        args.push("theme");
                    }
                    args.push(theme);
                }
                _ => unreachable!(),
            }
            Invocation::new(crate::paths::PKEXEC, &args)
        }
        Request::DisplayStatus => Invocation::new(
            crate::paths::PKEXEC,
            &[
                "--disable-internal-agent",
                crate::paths::WROS_DISPLAY,
                "status",
            ],
        ),
        Request::DisplayPreview(choice) => {
            choice.validate()?;
            Invocation::new(
                crate::paths::PKEXEC,
                &[
                    "--disable-internal-agent",
                    crate::paths::WROS_DISPLAY,
                    "preview",
                    if choice.enabled { "on" } else { "off" },
                    &choice.kelvin.to_string(),
                    &choice.strength.to_string(),
                ],
            )
        }
        Request::DisplayConfirm(token) | Request::DisplayCancel(token) => {
            crate::display::validate_token(token)?;
            Invocation::new(
                crate::paths::PKEXEC,
                &[
                    "--disable-internal-agent",
                    crate::paths::WROS_DISPLAY,
                    if matches!(request, Request::DisplayConfirm(_)) {
                        "confirm"
                    } else {
                        "cancel"
                    },
                    token,
                ],
            )
        }
    })
}
