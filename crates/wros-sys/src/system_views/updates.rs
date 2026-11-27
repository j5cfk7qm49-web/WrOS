//! Decode update and recovery replies and validate update inputs.
use super::label;
use crate::{
    RecoveryInfo, UpdateApplied, UpdateInfo, UpdatePackage, UpdateReview, UpdateStaged, UpdateState,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

pub fn updates(output: &str) -> Result<UpdateInfo> {
    ensure!(!output.trim().is_empty(), "empty APT simulation reply");
    let mut packages = Vec::new();
    let mut removals = Vec::new();
    for line in output.lines() {
        let target = if let Some(rest) = line.strip_prefix("Inst ") {
            Some((rest, &mut packages))
        } else {
            line.strip_prefix("Remv ").map(|rest| (rest, &mut removals))
        };
        if let Some((rest, list)) = target {
            let name = rest
                .split_whitespace()
                .next()
                .context("invalid APT package record")?;
            ensure!(
                name.len() <= 255
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"+.-:".contains(&c)),
                "invalid APT package name"
            );
            ensure!(list.len() < 1024, "APT plan exceeds limit");
            list.push(name.into());
        }
    }
    let status = format!(
        "Cached APT simulation: {} installations/upgrades, {} removals. Repository freshness and recovery are unchecked.",
        packages.len(),
        removals.len()
    );
    Ok(UpdateInfo {
        count: packages.len(),
        packages,
        removals,
        status,
    })
}

pub fn recovery_status(output: &str) -> Result<RecoveryInfo> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawRecovery {
        root_filesystem: String,
        root_device: String,
        separate_home: bool,
        rollback_capable: bool,
        rollback_strategy: String,
        interrupted_dpkg: bool,
        unconfigured_packages: Vec<String>,
        staged_update: bool,
        staged_packages: Vec<String>,
        update_state: UpdateState,
        #[serde(default)]
        previous_version: Option<String>,
        #[serde(default)]
        probation: bool,
        #[serde(default)]
        went_back: bool,
        #[serde(default)]
        went_back_automatically: bool,
        status: String,
    }
    let raw: RawRecovery =
        serde_json::from_str(output).context("invalid wros-update recovery reply")?;
    ensure!(
        !raw.root_filesystem.is_empty() && raw.root_filesystem.len() <= 64,
        "invalid root filesystem name"
    );
    label(&raw.root_filesystem)?;
    label(&raw.root_device)?;
    label(&raw.rollback_strategy)?;
    label(&raw.status)?;
    ensure!(
        raw.unconfigured_packages.len() <= 512 && raw.staged_packages.len() <= 512,
        "recovery package list exceeds limit"
    );
    for pkg in &raw.unconfigured_packages {
        label(pkg)?;
    }
    for pkg in &raw.staged_packages {
        label(pkg)?;
    }
    Ok(RecoveryInfo {
        root_filesystem: raw.root_filesystem,
        root_device: raw.root_device,
        separate_home: raw.separate_home,
        rollback_capable: raw.rollback_capable,
        rollback_strategy: raw.rollback_strategy,
        interrupted_dpkg: raw.interrupted_dpkg,
        unconfigured_packages: raw.unconfigured_packages,
        staged_update: raw.staged_update,
        staged_packages: raw.staged_packages,
        update_state: raw.update_state,
        previous_version: match raw.previous_version {
            Some(version) => {
                label(&version)?;
                Some(version)
            }
            None => None,
        },
        probation: raw.probation,
        went_back: raw.went_back,
        went_back_automatically: raw.went_back && raw.went_back_automatically,
        status: raw.status,
    })
}

/// Parse a helper reply as one JSON object and map a reported failure to an
/// error that carries the helper's own message.
fn helper_object(output: &str, verb: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
    let value: serde_json::Value =
        serde_json::from_str(output).with_context(|| format!("invalid {verb} reply"))?;
    let serde_json::Value::Object(object) = value else {
        anyhow::bail!("invalid {verb} reply: not a JSON object");
    };
    let status = object
        .get("status")
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("missing {verb} status"))?;
    if status == "failed" {
        let message = object
            .get("message")
            .and_then(serde_json::Value::as_str)
            .filter(|m| !m.is_empty() && label(m).is_ok())
            .unwrap_or("the helper reported failure without a message");
        anyhow::bail!("{message}");
    }
    Ok(object)
}

pub fn update_action_message(output: &str, expected_statuses: &[&str]) -> Result<String> {
    let object = helper_object(output, "wros-update")?;
    let status = object["status"].as_str().unwrap_or_default();
    ensure!(
        expected_statuses.contains(&status),
        "unexpected wros-update transaction status"
    );
    let message = object
        .get("message")
        .and_then(serde_json::Value::as_str)
        .context("missing wros-update message")?;
    ensure!(!message.is_empty(), "empty wros-update message");
    label(message)?;
    Ok(message.to_string())
}

fn lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A review identity is a lowercase SHA-256 hex digest.
pub fn validate_review_id(id: &str) -> Result<()> {
    ensure!(
        lower_hex(id, 64),
        "update review identity must be 64 lowercase hexadecimal characters"
    );
    Ok(())
}

fn short_text(value: &str, what: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
        "invalid update {what}"
    );
    Ok(())
}

fn package_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 255
            && name
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric())
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+.-".contains(&c)),
        "invalid update package name"
    );
    Ok(())
}

fn package_version(version: &str) -> Result<()> {
    ensure!(
        !version.is_empty()
            && version.len() <= 128
            && version
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric())
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"+.-:~".contains(&c)),
        "invalid update package version"
    );
    Ok(())
}

/// Strict parse of `wros-update review`.
pub fn update_review(output: &str) -> Result<UpdateReview> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawPackage {
        name: String,
        from_version: Option<String>,
        to_version: String,
        action: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawReview {
        status: String,
        review_id: String,
        signer: String,
        suite: String,
        date: String,
        valid_until: String,
        packages: Vec<RawPackage>,
        removals: Vec<String>,
        backup_recommended: bool,
        // From the NixOS updater (rc14 on): the system the update installs, and how many store
        // paths it carries. rc14 and rc15 refused these, so no update reached a laptop
        // through the app.
        #[serde(default)]
        toplevel: Option<String>,
        #[serde(default)]
        closure_paths: Option<u64>,
    }
    let object = helper_object(output, "wros-update review")?;
    let raw: RawReview = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update review reply")?;
    ensure!(raw.status == "reviewed", "unexpected review status");
    nix_toplevel(raw.toplevel.as_deref())?;
    ensure!(
        raw.closure_paths.is_none_or(|n| n <= 65_536),
        "update store path count exceeds limit"
    );
    validate_review_id(&raw.review_id)?;
    ensure!(
        raw.signer.len() == 40 && raw.signer.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid update signer fingerprint"
    );
    short_text(&raw.suite, "suite")?;
    short_text(&raw.date, "date")?;
    short_text(&raw.valid_until, "expiry")?;
    ensure!(
        raw.packages.len() <= 1024 && raw.removals.len() <= 1024,
        "update package list exceeds limit"
    );
    let mut packages: Vec<UpdatePackage> = Vec::with_capacity(raw.packages.len());
    for p in raw.packages {
        package_name(&p.name)?;
        package_version(&p.to_version)?;
        match (p.action.as_str(), &p.from_version) {
            ("install", None) => {}
            // "keep": a Debian dependency already installed at a newer version stays (#117).
            ("upgrade" | "reinstall" | "keep", Some(from)) => package_version(from)?,
            _ => anyhow::bail!("update package action does not match its previous version"),
        }
        ensure!(
            !packages.iter().any(|q| q.name == p.name),
            "duplicate update package"
        );
        packages.push(UpdatePackage {
            name: p.name,
            from_version: p.from_version,
            to_version: p.to_version,
            action: p.action,
        });
    }
    for name in &raw.removals {
        package_name(name)?;
        ensure!(
            !packages.iter().any(|p| &p.name == name),
            "update package is both changed and removed"
        );
    }
    Ok(UpdateReview {
        review_id: raw.review_id,
        signer: raw.signer,
        suite: raw.suite,
        date: raw.date,
        valid_until: raw.valid_until,
        packages,
        removals: raw.removals,
        backup_recommended: raw.backup_recommended,
    })
}

/// Strict parse of `wros-update-stage`. The helper must echo the identity the
/// writer approved; anything else is not a staged update.
pub fn update_staged(output: &str, expected_review_id: &str) -> Result<UpdateStaged> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawStaged {
        status: String,
        transaction_id: String,
        review_id: String,
        packages: usize,
        restart_required: bool,
        // The NixOS updater names the system it staged.
        #[serde(default)]
        toplevel: Option<String>,
    }
    let object = helper_object(output, "wros-update-stage")?;
    let raw: RawStaged = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-stage reply")?;
    ensure!(raw.status == "staged", "unexpected staging status");
    nix_toplevel(raw.toplevel.as_deref())?;
    ensure!(
        !raw.transaction_id.is_empty()
            && raw.transaction_id.len() <= 64
            && raw
                .transaction_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "invalid update transaction identity"
    );
    validate_review_id(&raw.review_id)?;
    ensure!(
        raw.review_id == expected_review_id,
        "the helper staged a different update than the one you reviewed"
    );
    ensure!(raw.packages <= 1024, "update package count exceeds limit");
    Ok(UpdateStaged {
        transaction_id: raw.transaction_id,
        review_id: raw.review_id,
        packages: raw.packages,
        restart_required: raw.restart_required,
    })
}

/// Strict parse of `wros-update-rollback`: going back to the kept previous version was staged.
pub fn update_gone_back(output: &str) -> Result<UpdateStaged> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawGoneBack {
        status: String,
        transaction_id: String,
        review_id: String,
        packages: usize,
        restart_required: bool,
        rollback: bool,
        // The NixOS updater names the generation it goes back to.
        #[serde(default)]
        toplevel: Option<String>,
    }
    let object = helper_object(output, "wros-update-rollback")?;
    let raw: RawGoneBack = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-rollback reply")?;
    nix_toplevel(raw.toplevel.as_deref())?;
    ensure!(
        raw.status == "staged" && raw.rollback,
        "going back was not staged"
    );
    ensure!(
        valid_transaction_id(&raw.transaction_id),
        "invalid update transaction identity"
    );
    validate_review_id(&raw.review_id)?;
    ensure!(raw.packages <= 1024, "update package count exceeds limit");
    Ok(UpdateStaged {
        transaction_id: raw.transaction_id,
        review_id: raw.review_id,
        packages: raw.packages,
        restart_required: raw.restart_required,
    })
}

/// A NixOS system the updater names, when it names one: a plain store path, nothing else.
fn nix_toplevel(toplevel: Option<&str>) -> Result<()> {
    if let Some(path) = toplevel {
        let name = path.strip_prefix("/nix/store/").unwrap_or_default();
        let (hash, rest) = name.split_at(name.len().min(32));
        ensure!(
            hash.len() == 32
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
                && rest.starts_with('-')
                && rest.len() > 1
                && rest.len() <= 200
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._+?=".contains(&b)),
            "invalid update system path"
        );
    }
    Ok(())
}

fn valid_transaction_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

/// Strict parse of `wros-update-apply`: only a completed install counts.
pub fn update_applied(output: &str) -> Result<UpdateApplied> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawApplied {
        status: String,
        transaction_id: String,
        // Debian named the packages it installed; NixOS names the system and its version.
        #[serde(default)]
        packages: Vec<String>,
        #[serde(default)]
        toplevel: Option<String>,
        #[serde(default)]
        version: Option<String>,
    }
    let object = helper_object(output, "wros-update-apply")?;
    let mut raw: RawApplied = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-apply reply")?;
    ensure!(raw.status == "completed", "the update was not installed");
    nix_toplevel(raw.toplevel.as_deref())?;
    if let Some(version) = raw.version.take() {
        package_version(&version)?;
        if raw.packages.is_empty() {
            raw.packages.push(format!("wros {version}"));
        }
    }
    ensure!(
        valid_transaction_id(&raw.transaction_id),
        "invalid update transaction identity"
    );
    ensure!(
        raw.packages.len() <= 1024
            && raw
                .packages
                .iter()
                .all(|p| p.len() <= 200 && p.chars().all(|c| c.is_ascii_graphic() || c == ' ')),
        "invalid installed package list"
    );
    Ok(UpdateApplied {
        transaction_id: raw.transaction_id,
        packages: raw.packages,
    })
}

/// Strict parse of `wros-update-media mount`.
/// `wros-update check-online`: a known status, and a version only when one was read.
pub fn online_update(output: &str) -> Result<crate::OnlineUpdate> {
    let object = helper_object(output, "wros-update check-online")?;
    let raw: crate::OnlineUpdate = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update check-online reply")?;
    ensure!(
        [
            "available",
            "needs-earlier-update",
            "current",
            "clock",
            "expired",
            "unreachable",
            "unpublished",
            "invalid"
        ]
        .contains(&raw.status.as_str()),
        "unknown online update status"
    );
    for value in [&raw.version, &raw.installed].into_iter().flatten() {
        ensure!(
            !value.is_empty()
                && value.len() <= 64
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".+~:-".contains(c)),
            "invalid version in online update reply"
        );
    }
    ensure!(
        !matches!(raw.status.as_str(), "available" | "needs-earlier-update")
            || raw.version.is_some(),
        "online update reply names no version"
    );
    if raw.status == "needs-earlier-update" {
        let needs = raw
            .needs
            .as_deref()
            .context("online update reply names no baseline")?;
        ensure!(!needs.is_empty(), "online update reply names no baseline");
        label(needs)?;
    }
    label(&raw.clock)?;
    Ok(raw)
}

/// `wros-update-fetch`: the verified folder, which must be in the online cache.
pub fn online_fetch(output: &str) -> Result<String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawFetch {
        status: String,
        path: String,
        version: String,
    }
    let object = helper_object(output, "wros-update-fetch")?;
    let raw: RawFetch = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-fetch reply")?;
    ensure!(raw.status == "fetched", "wros-update-fetch did not fetch");
    label(&raw.version)?;
    validate_update_bundle_path(&raw.path)?;
    ensure!(
        raw.path.starts_with(ONLINE_UPDATE_PREFIX),
        "downloaded update is outside the online update cache"
    );
    Ok(raw.path)
}

pub fn update_media(output: &str) -> Result<Option<String>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawMedia {
        status: String,
        path: Option<String>,
        message: Option<String>,
    }
    let object = helper_object(output, "wros-update-media")?;
    let raw: RawMedia = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-media reply")?;
    if let Some(message) = &raw.message {
        label(message)?;
    }
    match (raw.status.as_str(), raw.path) {
        ("none", None) => Ok(None),
        ("mounted", Some(path)) => {
            validate_update_bundle_path(&path)?;
            ensure!(
                path.starts_with(MANAGED_MEDIA_PREFIX),
                "mounted update media is outside the managed mount directory"
            );
            Ok(Some(path))
        }
        _ => anyhow::bail!("unexpected update media status"),
    }
}

/// Directory in which the privileged media verb mounts `WROS-UPDATE` media.
pub const MANAGED_MEDIA_PREFIX: &str = "/run/wros-update/media/";

/// Where the root fetch helper publishes a verified online update.
pub const ONLINE_UPDATE_PREFIX: &str = "/var/cache/wros-update/online/";
/// Prefixes under which an update bundle path is accepted.
pub const UPDATE_MEDIA_PREFIXES: [&str; 5] = [
    MANAGED_MEDIA_PREFIX,
    "/media/",
    "/run/media/",
    "/mnt/",
    ONLINE_UPDATE_PREFIX,
];

pub fn validate_update_bundle_path(path: &str) -> Result<()> {
    ensure!(
        path.starts_with('/')
            && path != "/"
            && path.len() <= 512
            && !path.chars().any(char::is_control)
            && !std::path::Path::new(path)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "update bundle path must be an absolute directory path without '..' or control characters"
    );
    // Only removable-media locations are accepted. System, runtime-state and
    // home directories are never update sources, and the path must be written
    // canonically so what the writer sees is what the helper receives.
    ensure!(
        UPDATE_MEDIA_PREFIXES
            .iter()
            .any(|prefix| path.len() > prefix.len() && path.starts_with(prefix))
            && !path.ends_with('/')
            && !path.contains("//")
            && !path.split('/').any(|part| part == "."),
        "update bundle must be a folder on update media under /run/wros-update/media, /media, /run/media or /mnt, or a downloaded WrOS update"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_package_plan_includes_removals_and_never_claims_freshness() {
        let plan=updates("Inst editor [1] (2 Debian-Security)\nRemv old-editor [1]\nConf editor (2 Debian-Security)").unwrap();
        assert_eq!(plan.packages, vec!["editor"]);
        assert_eq!(plan.removals, vec!["old-editor"]);
        assert!(plan.status.contains("unchecked"));
        assert!(
            !updates("Reading package lists...")
                .unwrap()
                .status
                .contains("up to date")
        );
        assert!(updates("").is_err());
    }
    #[test]
    fn update_review_ids_and_media_prefixes_are_strict() {
        assert!(validate_review_id(&"0123456789abcdef".repeat(4)).is_ok());
        for bad in [
            "",
            "ABCDEF",
            &"A".repeat(64),
            &"a".repeat(65),
            &"a".repeat(63),
        ] {
            assert!(validate_review_id(bad).is_err(), "{bad:?}");
        }
        // The engine downloads online updates here; it reviews that folder like media.
        assert!(
            validate_update_bundle_path("/var/cache/wros-update/online/0123456789abcdef").is_ok()
        );
        assert!(validate_update_bundle_path("/var/cache/wros-update/x").is_err());
        for prefix in UPDATE_MEDIA_PREFIXES {
            assert!(validate_update_bundle_path(&format!("{prefix}x")).is_ok());
            // The prefix directory itself is not a bundle.
            assert!(validate_update_bundle_path(prefix).is_err());
            assert!(validate_update_bundle_path(prefix.trim_end_matches('/')).is_err());
        }
        for bad in [
            "/run/wros-update/staging/x",
            "/var/lib/wros-update/x",
            "/media-extra/x",
            "/mnt/./x",
            "/mnt/x/",
            "/mnt//x",
        ] {
            assert!(validate_update_bundle_path(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn recovery_status_accepts_ext4_with_a_previous_version() {
        let ext4_json = r#"{
            "root_filesystem":"ext4",
            "root_device":"/dev/mapper/root",
            "separate_home":false,
            "rollback_capable":true,
            "rollback_strategy":"The previous WrOS version is kept in the start menu.",
            "interrupted_dpkg":false,
            "unconfigured_packages":[],
            "staged_update":false,
            "staged_packages":[],
            "update_state":"idle",
            "previous_version":"0.1.0~rc18",
            "status":"clean"
        }"#;
        let rec = recovery_status(ext4_json).unwrap();
        assert_eq!(rec.root_filesystem, "ext4");
        assert_eq!(rec.update_state, UpdateState::Idle);
        assert!(rec.rollback_capable);
        assert_eq!(rec.previous_version.as_deref(), Some("0.1.0~rc18"));
        // The journal state is mandatory and limited to the documented set.
        assert!(recovery_status(&ext4_json.replace("\"update_state\":\"idle\",", "")).is_err());
        assert!(recovery_status(&ext4_json.replace("\"idle\"", "\"rolled-back\"")).is_err());
    }

    #[test]
    fn online_update_parses_the_required_baseline() {
        let reply = r#"{"status":"needs-earlier-update","version":"0.1.0~rc18",
            "installed":"0.1.0~rc16","clock":"2026-11-18 12:00","needs":"0.1.0~rc17"}"#;
        let update = online_update(reply).unwrap();
        assert_eq!(update.status, "needs-earlier-update");
        assert_eq!(update.version.as_deref(), Some("0.1.0~rc18"));
        assert_eq!(update.needs.as_deref(), Some("0.1.0~rc17"));
        assert!(online_update(&reply.replace(r#","needs":"0.1.0~rc17""#, "")).is_err());
    }

    /// What the NixOS updater (nixos/scripts/wros-update-engine.py) prints for review, stage and
    /// going back. rc14 and rc15 refused the extra fields, so no update reached a laptop through
    /// the app; os/tests/test_update_reply_contract.py keeps the two sides in step.
    #[test]
    fn nixos_update_replies_are_accepted() {
        let id = "0123456789abcdef".repeat(4);
        let top = "/nix/store/62l926jbq7vm5vmq6476sgg42pism5ls-nixos-system-unnamed-0.1.0-rc15";
        let review = format!(
            r#"{{"status": "reviewed", "review_id": "{id}", "signer": "B9F5B97C9370239CC07B425F2CB426FC6D27CA65",
            "suite": "wros-nixos-updates", "date": "2026-11-16T19:05:35+00:00",
            "valid_until": "2026-12-16T19:05:35+00:00",
            "packages": [{{"name": "wros", "from_version": "0.1.0~rc14", "to_version": "0.1.0~rc15", "action": "upgrade"}}],
            "toplevel": "{top}", "closure_paths": 44, "removals": [], "backup_recommended": true}}"#
        );
        let reviewed = update_review(&review).unwrap();
        assert_eq!(reviewed.packages[0].to_version, "0.1.0~rc15");
        let staged = format!(
            r#"{{"status": "staged", "transaction_id": "tx-1", "review_id": "{id}", "packages": 1,
            "toplevel": "{top}", "restart_required": true}}"#
        );
        assert!(update_staged(&staged, &id).unwrap().restart_required);
        let back = format!(
            r#"{{"status": "staged", "transaction_id": "tx-2", "review_id": "{id}", "packages": 1,
            "toplevel": "{top}", "restart_required": true, "rollback": true}}"#
        );
        assert!(update_gone_back(&back).is_ok());
        let applied = format!(
            r#"{{"status": "completed", "transaction_id": "tx-1", "toplevel": "{top}", "version": "0.1.0~rc15"}}"#
        );
        assert_eq!(
            update_applied(&applied).unwrap().packages,
            ["wros 0.1.0~rc15"]
        );
        // Still strict: a system path that is not a plain store path, or a field nobody sends.
        for bad in [
            "/etc/passwd",
            "/nix/store/short-x",
            "/nix/store/62l926jbq7vm5vmq6476sgg42pism5ls-a/../b",
        ] {
            assert!(update_review(&review.replace(top, bad)).is_err(), "{bad}");
        }
        assert!(
            update_review(&review.replace("\"removals\"", "\"surprise\": 1, \"removals\""))
                .is_err()
        );
    }
}
