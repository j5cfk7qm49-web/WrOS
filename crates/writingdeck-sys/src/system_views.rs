//! Bounded read-only views of maintained Debian services. None of these
//! parsers infer successful authentication, backup, update or recovery.
use crate::{
    LocalAccount, RecoveryInfo, ResticSnapshot, SavedWifi, StorageDevice, UpdateInfo,
    UpdatePackage, UpdateReview, UpdateStaged, UpdateState, WifiNetwork,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

fn label(value: &str) -> Result<()> {
    ensure!(
        value.len() <= 4096 && !value.chars().any(char::is_control),
        "invalid system label"
    );
    Ok(())
}

// nmcli's documented terse escaping is backslash-colon / backslash-backslash.
// Do not trim SSIDs or profile names: leading/trailing spaces are significant.
fn fields(line: &str, count: usize) -> Result<Vec<String>> {
    let mut parts = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let escaped = chars.next().context("incomplete nmcli escape")?;
                ensure!(matches!(escaped, ':' | '\\'), "unknown nmcli escape");
                parts.last_mut().unwrap().push(escaped);
            }
            ':' => parts.push(String::new()),
            c => parts.last_mut().unwrap().push(c),
        }
    }
    ensure!(parts.len() == count, "invalid nmcli field count");
    for part in &parts {
        label(part)?;
    }
    Ok(parts)
}

pub fn wifi_scan(output: &str) -> Result<Vec<WifiNetwork>> {
    let mut networks = Vec::new();
    for (index, line) in output.lines().enumerate() {
        ensure!(index < 512, "Wi-Fi list exceeds limit");
        let p = fields(line, 4)?;
        ensure!(
            matches!(p[0].as_str(), "*" | ""),
            "invalid active Wi-Fi flag"
        );
        let signal: u8 = p[2].parse().context("invalid Wi-Fi signal")?;
        ensure!(signal <= 100, "invalid Wi-Fi signal");
        if p[1].is_empty() {
            continue;
        } // hidden networks are not enrollment targets
        networks.push(WifiNetwork {
            ssid: p[1].clone(),
            signal,
            security: p[3].clone(),
            in_use: p[0] == "*",
        });
    }
    Ok(networks)
}

pub fn saved_wifi(output: &str) -> Result<Vec<SavedWifi>> {
    let mut saved = Vec::new();
    for (index, line) in output.lines().enumerate() {
        ensure!(index < 512, "saved connection list exceeds limit");
        let p = fields(line, 3)?;
        if !matches!(p[2].as_str(), "802-11-wireless" | "wifi") {
            continue;
        }
        ensure!(
            p[0].len() == 36
                && p[0].bytes().enumerate().all(|(i, c)| {
                    if matches!(i, 8 | 13 | 18 | 23) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                }),
            "invalid saved connection identity"
        );
        ensure!(
            !saved.iter().any(|s: &SavedWifi| s.uuid == p[0]),
            "duplicate saved connection identity"
        );
        saved.push(SavedWifi {
            uuid: p[0].clone(),
            name: p[1].clone(),
        });
    }
    Ok(saved)
}

pub fn local_accounts(passwd: &str, current_uid: u32) -> Result<Vec<LocalAccount>> {
    let mut accounts = Vec::new();
    for line in passwd.lines() {
        let p: Vec<_> = line.split(':').collect();
        ensure!(p.len() == 7, "invalid local account record");
        let uid: u32 = p[2].parse().context("invalid local account UID")?;
        if !(1000..65534).contains(&uid) || p[6].ends_with("/nologin") || p[6].ends_with("/false") {
            continue;
        }
        ensure!(
            !p[0].is_empty()
                && p[0].len() <= 64
                && p[0]
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c)),
            "invalid local account name"
        );
        ensure!(
            p[5].starts_with('/') && p[6].starts_with('/'),
            "invalid local account path"
        );
        label(p[5])?;
        ensure!(accounts.len() < 512, "local account list exceeds limit");
        accounts.push(LocalAccount {
            username: p[0].into(),
            uid,
            home: p[5].into(),
            is_current: uid == current_uid,
        });
    }
    accounts.sort_by(|a, b| a.username.cmp(&b.username));
    Ok(accounts)
}

pub fn storage(output: &str) -> Result<Vec<StorageDevice>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Mounts {
        filesystems: Vec<Mount>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Mount {
        source: String,
        target: String,
        size: serde_json::Value,
        used: serde_json::Value,
        avail: serde_json::Value,
        #[serde(rename = "use%")]
        percent: String,
    }
    fn bytes(v: &serde_json::Value) -> Result<u64> {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .context("invalid filesystem byte count")
    }
    let mounts: Mounts = serde_json::from_str(output).context("invalid findmnt reply")?;
    ensure!(
        mounts.filesystems.len() <= 512,
        "filesystem list exceeds limit"
    );
    let mut devices = Vec::new();
    for m in mounts.filesystems {
        label(&m.source)?;
        label(&m.target)?;
        let percent: u8 = m
            .percent
            .strip_suffix('%')
            .context("invalid filesystem percentage")?
            .parse()?;
        ensure!(percent <= 100, "invalid filesystem percentage");
        devices.push(StorageDevice {
            filesystem: m.source,
            mountpoint: m.target,
            total_bytes: bytes(&m.size)?,
            used_bytes: bytes(&m.used)?,
            available_bytes: bytes(&m.avail)?,
            use_percent: percent,
        });
    }
    Ok(devices)
}

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
    if raw.root_filesystem == "ext4" {
        ensure!(
            !raw.rollback_capable,
            "ext4 root filesystem cannot claim automatic snapshot rollback"
        );
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
    }
    let object = helper_object(output, "wros-update review")?;
    let raw: RawReview = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update review reply")?;
    ensure!(raw.status == "reviewed", "unexpected review status");
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
            ("upgrade" | "reinstall", Some(from)) => package_version(from)?,
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
    }
    let object = helper_object(output, "wros-update-stage")?;
    let raw: RawStaged = serde_json::from_value(serde_json::Value::Object(object))
        .context("invalid wros-update-stage reply")?;
    ensure!(raw.status == "staged", "unexpected staging status");
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

/// Strict parse of `wros-update-media mount`.
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

pub fn restic_snapshots(output: &str) -> Result<Vec<ResticSnapshot>> {
    ensure!(
        output.len() <= 1_048_576,
        "restic snapshot output exceeds limit"
    );
    #[derive(Deserialize)]
    struct RawSnapshot {
        short_id: Option<String>,
        id: String,
        time: String,
        paths: Vec<String>,
        hostname: String,
    }
    let parsed: Vec<RawSnapshot> =
        serde_json::from_str(output).context("invalid restic snapshots JSON")?;
    ensure!(parsed.len() <= 512, "restic snapshot list exceeds limit");
    let mut out = Vec::new();
    for item in parsed {
        let snap_id = item.short_id.unwrap_or(item.id);
        ensure!(
            (8..=64).contains(&snap_id.len()) && snap_id.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid restic snapshot ID"
        );
        label(&item.time)?;
        label(&item.hostname)?;
        ensure!(
            !item.paths.is_empty() && item.paths.len() <= 64,
            "invalid restic snapshot paths"
        );
        for p in &item.paths {
            ensure!(p.starts_with('/'), "restic path must be absolute");
            label(p)?;
        }
        out.push(ResticSnapshot {
            id: snap_id,
            time: item.time,
            paths: item.paths,
            hostname: item.hostname,
        });
    }
    Ok(out)
}

pub fn validate_saved_wifi_uuid(uuid: &str) -> Result<()> {
    ensure!(
        uuid.len() == 36
            && uuid.bytes().enumerate().all(|(i, c)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            }),
        "saved Wi-Fi profile operation requires an exact 36-char UUID"
    );
    Ok(())
}

pub fn validate_wifi_ssid(ssid: &str) -> Result<()> {
    ensure!(
        !ssid.is_empty()
            && ssid.len() <= 32
            && !ssid.starts_with('-')
            && !ssid.chars().any(char::is_control),
        "Wi-Fi network name must be 1–32 UTF-8 bytes without control characters or a leading dash"
    );
    Ok(())
}

pub fn validate_wifi_password(password: &str) -> Result<()> {
    ensure!(
        ((8..=63).contains(&password.len()) && !password.chars().any(char::is_control))
            || (password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit())),
        "Wi-Fi password must be 8–63 bytes, or an exact 64-digit hexadecimal WPA key"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wifi_preserves_spaces_and_decodes_escaped_names_without_guessing() {
        let rows = wifi_scan("*: Desk\\:Main\\\\West :91:WPA2\n:--:10:--").unwrap();
        assert_eq!(rows[0].ssid, " Desk:Main\\West ");
        assert!(rows[0].in_use);
        assert_eq!(rows[1].ssid, "--"); // this can be a real SSID
        for bad in [
            "*:Desk:bogus:WPA2",
            "*:Desk:101:WPA2",
            "*:Desk\\q:90:WPA2",
            "*:a:90:WPA2:extra",
        ] {
            assert!(wifi_scan(bad).is_err());
        }
    }
    #[test]
    fn saved_names_can_collide_but_identity_is_an_actual_uuid() {
        let a = "00000000-0000-4000-8000-000000000001";
        let b = "00000000-0000-4000-8000-000000000002";
        let rows = saved_wifi(&format!(
            "{a}: Desk\\:Main :802-11-wireless\n{b}: Desk\\:Main :wifi"
        ))
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].uuid, rows[1].uuid);
        assert_eq!(rows[0].name, " Desk:Main ");
        assert!(saved_wifi("not-a-uuid:Home:wifi").is_err());
        assert!(saved_wifi(&format!("{a}:Home:wifi\n{a}:Home:wifi")).is_err());
    }
    #[test]
    fn absent_accounts_are_empty_and_nonlogin_system_accounts_are_excluded() {
        assert!(local_accounts("", 1000).unwrap().is_empty());
        let rows=local_accounts("root:x:0:0::/root:/bin/bash\nsvc:x:1001:1001::/home/svc:/usr/sbin/nologin\nactual:x:1002:1002::/home/actual:/bin/bash",1002).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].username, "actual");
        assert!(rows[0].is_current);
        assert!(local_accounts("malformed", 1000).is_err());
    }
    #[test]
    fn storage_json_preserves_space_containing_mounts_and_rejects_partial_data() {
        let good = r#"{"filesystems":[{"source":"/dev/mapper/root","target":"/media/My drafts","size":1000,"used":250,"avail":"750","use%":"25%"}]}"#;
        let rows = storage(good).unwrap();
        assert_eq!(rows[0].mountpoint, "/media/My drafts");
        assert_eq!(rows[0].available_bytes, 750);
        assert!(storage(&good.replace("\"750\"", "null")).is_err());
        assert!(storage("not JSON").is_err());
    }
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
    fn restic_json_and_wifi_uuid_validators_enforce_strict_identities() {
        let json = r#"[{"id":"a8f3b9c100112233","short_id":"a8f3b9c1","time":"2026-10-29T12:00:00Z","paths":["/home/writer/novels"],"hostname":"wros-laptop"}]"#;
        let snaps = restic_snapshots(json).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].id, "a8f3b9c1");
        assert!(
            restic_snapshots(r#"[{"id":"not-hex","time":"now","paths":["/home"],"hostname":"h"}]"#)
                .is_err()
        );
        assert!(validate_saved_wifi_uuid("00000000-0000-4000-8000-000000000001").is_ok());
        assert!(validate_saved_wifi_uuid("ambiguous profile").is_err());
        assert!(validate_wifi_ssid(" Desk:Main\\West ").is_ok());
        assert!(validate_wifi_ssid("").is_err());
        assert!(validate_wifi_ssid("--ask").is_err());
        assert!(validate_wifi_ssid(&"a".repeat(33)).is_err());
        assert!(validate_wifi_password("correct-horse-battery").is_ok());
        assert!(validate_wifi_password("short").is_err());
        assert!(validate_wifi_password("bad\npassword").is_err());
        assert!(validate_wifi_password(&"a".repeat(63)).is_ok());
        assert!(validate_wifi_password(&"A".repeat(64)).is_ok());
        assert!(validate_wifi_password(&"z".repeat(64)).is_err());
        assert!(validate_wifi_password(&"a".repeat(65)).is_err());
        assert!(validate_update_bundle_path("/media/usb/wros-updates").is_ok());
        assert!(validate_update_bundle_path("relative/path").is_err());
        assert!(validate_update_bundle_path("/").is_err());
        assert!(validate_update_bundle_path("/media/../etc").is_err());
        let ext4_json = r#"{"root_filesystem":"ext4","root_device":"/dev/mapper/root","separate_home":false,"rollback_capable":false,"rollback_strategy":"ext4 has no snapshot rollback","interrupted_dpkg":false,"unconfigured_packages":[],"staged_update":false,"staged_packages":[],"update_state":"idle","status":"clean"}"#;
        let rec = recovery_status(ext4_json).unwrap();
        assert_eq!(rec.root_filesystem, "ext4");
        assert_eq!(rec.update_state, UpdateState::Idle);
        assert!(!rec.rollback_capable);
        // The journal state is mandatory and limited to the documented set.
        assert!(recovery_status(&ext4_json.replace("\"update_state\":\"idle\",", "")).is_err());
        assert!(recovery_status(&ext4_json.replace("\"idle\"", "\"rolled-back\"")).is_err());
        assert!(
            recovery_status(
                &ext4_json.replace("\"rollback_capable\":false", "\"rollback_capable\":true")
            )
            .is_err()
        );
    }
}
