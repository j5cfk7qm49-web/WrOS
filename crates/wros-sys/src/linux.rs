//! Conservative Linux adapter. No shell expansion, sudo, password arguments,
//! manuscript access, or implicit network connection. Requires an active local
//! logind session for authorized actions. Read-only status also works in dev mode.
use crate::{Backend, Reply, Request};
use anyhow::{Context, Result, bail};
use std::{fs, io::Read, path::PathBuf};

mod invocation;
mod runner;
mod snapshot;
#[cfg(test)]
mod tests;

pub use invocation::{
    APPLY_TIMEOUT_SECS, CANCEL_TIMEOUT_SECS, Invocation, MEDIA_TIMEOUT_SECS,
    ONLINE_CHECK_TIMEOUT_SECS, ONLINE_FETCH_TIMEOUT_SECS, REPAIR_TIMEOUT_SECS, REVIEW_TIMEOUT_SECS,
    STAGE_TIMEOUT_SECS, invocation,
};
use invocation::{active_wifi_connections, timezone_status};
pub use runner::{ProcessRunner, Runner};
pub use snapshot::{battery, brightness};

pub struct Linux<R = ProcessRunner> {
    pub runner: R,
    pub sys: PathBuf,
    pub etc: PathBuf,
    pub storage: PathBuf,
}
impl Default for Linux {
    fn default() -> Self {
        Self {
            runner: ProcessRunner,
            sys: "/sys".into(),
            etc: "/etc".into(),
            storage: "/home".into(),
        }
    }
}

fn scrub_bytes(buf: &mut [u8]) {
    for byte in buf {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

impl<R: Runner> Backend for Linux<R> {
    fn request(&mut self, request: Request) -> Result<Reply> {
        if request == Request::Refresh {
            return Ok(Reply::Snapshot(self.snapshot()));
        }
        if request == Request::ListLocalAccounts {
            let file =
                fs::File::open(self.etc.join("passwd")).context("reading local account list")?;
            let mut content = String::new();
            file.take(1_048_577).read_to_string(&mut content)?;
            if content.len() > 1_048_576 {
                bail!("local account list exceeds limit");
            }
            return Ok(Reply::LocalAccounts(crate::system_views::local_accounts(
                &content,
                unsafe { libc::geteuid() },
            )?));
        }
        if request == Request::GetGoogleStatus {
            return Ok(Reply::GoogleStatus(crate::GoogleAccountStatus {
                connected: false,
                email: None,
                status_message: "Google enrollment is not configured in this build.".into(),
            }));
        }
        if let Request::ChangePassword { current, new } = &request {
            let (Some(current), Some(new)) = (current.as_ref(), new.as_ref()) else {
                bail!("type your current password and a new one");
            };
            crate::system_views::validate_login_password(current.as_str())?;
            crate::system_views::validate_login_password(new.as_str())?;
            let cmd = invocation(&request)?;
            let mut input = format!("{}\n{}\n", current.as_str(), new.as_str()).into_bytes();
            let result = self.runner.run_with_input(&cmd, &input);
            scrub_bytes(&mut input);
            let output = result.map_err(|e| {
                let redacted = e
                    .to_string()
                    .replace(current.as_str(), "[hidden]")
                    .replace(new.as_str(), "[hidden]");
                anyhow::anyhow!("{redacted}")
            })?;
            return crate::system_views::password_reply(&output);
        }
        if let Request::ConnectWifi { ssid, password } = &request {
            let cmd = invocation(&request)?;
            let run_result = if let Some(secret) = password {
                let mut stdin_bytes = format!("{}\n", secret.as_str()).into_bytes();
                let res = self.runner.run_with_input(&cmd, &stdin_bytes);
                scrub_bytes(&mut stdin_bytes);
                res.map_err(|e| {
                    let redacted = e.to_string().replace(secret.as_str(), "[hidden]");
                    anyhow::anyhow!("{redacted}")
                })
            } else {
                self.runner.run(&cmd)
            };
            let output = run_result?;
            if output.contains("Error:") {
                bail!("Wi-Fi connection failed; check network credentials and signal");
            }
            let active_output = self.runner.run(&Invocation::new(
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
                    "no",
                ],
            ))?;
            let active = crate::system_views::wifi_scan(&active_output)?;
            if !active
                .iter()
                .any(|network| network.in_use && network.ssid == *ssid)
            {
                bail!(
                    "Requested Wi-Fi network could not be verified as active; check credentials and signal"
                );
            }
            return Ok(Reply::Message(format!(
                "Connected to Wi-Fi network {ssid}."
            )));
        }
        if let Request::ActivateSavedWifi(uuid) = &request {
            let cmd = invocation(&request)?;
            let output = self.runner.run(&cmd)?;
            if output.contains("Error:") {
                bail!("saved Wi-Fi profile could not be activated");
            }
            let active_output = self.runner.run(&active_wifi_connections())?;
            let active = crate::system_views::saved_wifi(&active_output)?;
            if !active.iter().any(|p| &p.uuid == uuid) {
                bail!("saved Wi-Fi profile activation could not be verified");
            }
            return Ok(Reply::Message(format!(
                "Connected saved Wi-Fi profile {uuid}."
            )));
        }
        if request == Request::DisconnectWifi {
            let active_output = self.runner.run(&invocation(&request)?)?;
            let active = crate::system_views::saved_wifi(&active_output)?;
            if active.is_empty() {
                bail!("no active Wi-Fi connection to disconnect");
            }
            for profile in &active {
                self.runner.run(&Invocation::new(
                    crate::paths::NMCLI,
                    &[
                        "--colors",
                        "no",
                        "connection",
                        "down",
                        "uuid",
                        &profile.uuid,
                    ],
                ))?;
            }
            return Ok(Reply::Message("Wi-Fi disconnected.".into()));
        }
        if let Request::ForgetWifi(uuid) = &request {
            let cmd = invocation(&request)?;
            self.runner.run(&cmd)?;
            return Ok(Reply::Message(format!(
                "Removed saved Wi-Fi profile {uuid}."
            )));
        }
        let output = self.runner.run(&invocation(&request)?)?;
        if matches!(
            request,
            Request::BootStatus | Request::BootTheme(_) | Request::BootScreen { .. }
        ) {
            let status: crate::boot::Status =
                serde_json::from_str(&output).context("invalid start-screen status")?;
            if let Request::BootScreen { enabled, .. } = request
                && status.enabled != enabled
            {
                bail!("the start-screen choice could not be confirmed");
            }
            return Ok(Reply::BootStatus(status));
        }
        if request == Request::ListTimezones {
            return Ok(Reply::Timezones(crate::timezone::catalog(&output)?));
        }
        if let Request::SetTimezone(expected) = &request {
            // A successful method call alone does not establish the current
            // value; another session may have changed it concurrently.
            let actual = self.runner.run(&timezone_status())?;
            crate::timezone::validate(&actual)?;
            if &actual != expected {
                bail!("timezone change could not be verified; refresh current status");
            }
            return Ok(Reply::Timezone(actual));
        }
        if let Request::SetLocalClock(expected) = &request {
            let expected_norm = crate::timezone::validate_local_datetime(expected)?;
            let actual = crate::system_views::parse_clock_reply(&output, &expected_norm)?;
            return Ok(Reply::LocalClock(actual));
        }
        if matches!(request, Request::UseNetworkTime) {
            crate::system_views::parse_network_time_reply(&output)?;
            return Ok(Reply::NetworkTime);
        }
        if matches!(
            request,
            Request::AudioStatus | Request::SetVolume(_) | Request::SetMute(_)
        ) {
            return Ok(Reply::Audio(crate::system_views::parse_sound_status(
                &output,
            )?));
        }
        if matches!(request, Request::DisplayStatus) {
            let status: crate::display::Status =
                serde_json::from_str(&output).context("invalid display status")?;
            status.validate()?;
            return Ok(Reply::DisplayStatus(status));
        }
        if matches!(request, Request::DisplayPreview(_)) {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Preview {
                token: String,
                seconds: u64,
            }
            let preview: Preview =
                serde_json::from_str(&output).context("invalid display preview reply")?;
            crate::display::validate_token(&preview.token)?;
            if preview.seconds == 0 || preview.seconds > 15 {
                bail!("display preview expired or invalid");
            }
            return Ok(Reply::DisplayPreview {
                token: preview.token,
                seconds: preview.seconds,
            });
        }
        if matches!(
            request,
            Request::DisplayConfirm(_) | Request::DisplayCancel(_)
        ) {
            let result: serde_json::Value =
                serde_json::from_str(&output).context("invalid display completion reply")?;
            let message = match (request, result["status"].as_str()) {
                (Request::DisplayConfirm(_), Some("committed")) => {
                    "Screen warmth kept for this display and future boots."
                }
                (Request::DisplayCancel(_), Some("cancelled")) => {
                    "Previous screen colors restored."
                }
                (Request::DisplayCancel(_), Some("no-preview")) => {
                    "Display preview has already finished."
                }
                _ => bail!("display transaction was not confirmed by the service"),
            };
            return Ok(Reply::Message(message.into()));
        }
        match &request {
            Request::ListImportMedia
            | Request::MountImportMedia(_)
            | Request::UnmountImportMedia => {
                let sticks: Vec<crate::ImportStick> =
                    serde_json::from_str(&output).context("invalid import media reply")?;
                if let Request::MountImportMedia(device) = &request
                    && (sticks.len() != 1
                        || sticks[0].device != *device
                        || sticks[0].mount.as_deref() != Some("/run/wros-import/media/1"))
                {
                    bail!("the USB stick was not mounted by the service");
                }
                if request == Request::UnmountImportMedia && !sticks.is_empty() {
                    bail!("the USB stick was not unmounted by the service");
                }
                return Ok(Reply::ImportMedia(sticks));
            }
            Request::ScanWifi => {
                return Ok(Reply::WifiScan(crate::system_views::wifi_scan(&output)?));
            }
            Request::ListSavedWifi => {
                return Ok(Reply::SavedWifi(crate::system_views::saved_wifi(&output)?));
            }
            Request::ListStorageDevices => {
                return Ok(Reply::StorageDevices(crate::system_views::storage(
                    &output,
                )?));
            }
            Request::CheckUpdates => {
                return Ok(Reply::Updates(crate::system_views::updates(&output)?));
            }
            Request::RecoveryStatus => {
                return Ok(Reply::Recovery(crate::system_views::recovery_status(
                    &output,
                )?));
            }
            Request::VerifyUpdateBundle { .. } => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["verified"],
                )?));
            }
            Request::ReviewUpdateBundle { .. } => {
                return Ok(Reply::UpdateReview(crate::system_views::update_review(
                    &output,
                )?));
            }
            Request::StageUpdates { review_id, .. } => {
                return Ok(Reply::UpdateStaged(crate::system_views::update_staged(
                    &output, review_id,
                )?));
            }
            Request::FindUpdateMedia => {
                return Ok(Reply::UpdateMedia(crate::system_views::update_media(
                    &output,
                )?));
            }
            Request::CheckOnlineUpdates => {
                return Ok(Reply::OnlineUpdate(crate::system_views::online_update(
                    &output,
                )?));
            }
            Request::FetchOnlineUpdate => {
                return Ok(Reply::OnlineUpdateFetched(
                    crate::system_views::online_fetch(&output)?,
                ));
            }
            Request::ApplyStagedUpdate => {
                return Ok(Reply::UpdateApplied(crate::system_views::update_applied(
                    &output,
                )?));
            }
            Request::CancelStagedUpdates => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["cancelled", "none"],
                )?));
            }
            Request::GoBackToPreviousVersion => {
                return Ok(Reply::UpdateStaged(crate::system_views::update_gone_back(
                    &output,
                )?));
            }
            Request::ReportUpdateReady => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["ready", "none", "busy"],
                )?));
            }
            Request::RecoverUpdates => {
                return Ok(Reply::Message(crate::system_views::update_action_message(
                    &output,
                    &["recovered", "clean"],
                )?));
            }
            _ => {}
        }
        if matches!(request, Request::KeyboardPreview(_)) {
            let v: serde_json::Value =
                serde_json::from_str(&output).context("invalid keyboard helper reply")?;
            let token = v["token"]
                .as_str()
                .context("missing keyboard token")?
                .to_string();
            invocation(&Request::KeyboardConfirm(token.clone()))?;
            let seconds = v["seconds"].as_u64().unwrap_or(15).min(15);
            return Ok(Reply::KeyboardPreview { token, seconds });
        }
        Ok(Reply::Message(if output.is_empty() {
            "request completed".into()
        } else {
            output
        }))
    }
}
