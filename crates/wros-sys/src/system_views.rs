//! Bounded read-only views of maintained Debian services. None of these
//! parsers infer successful authentication, backup, update or recovery.
use crate::{AudioStatus, LocalAccount, ResticSnapshot, SavedWifi, StorageDevice, WifiNetwork};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

mod updates;

pub use updates::{
    MANAGED_MEDIA_PREFIX, ONLINE_UPDATE_PREFIX, UPDATE_MEDIA_PREFIXES, online_fetch, online_update,
    recovery_status, update_action_message, update_applied, update_gone_back, update_media,
    update_review, update_staged, updates, validate_review_id, validate_update_bundle_path,
};

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
    let mut parts = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let escaped = chars.next().context("incomplete nmcli escape")?;
                ensure!(matches!(escaped, ':' | '\\'), "unknown nmcli escape");
                field.push(escaped);
            }
            ':' => parts.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    parts.push(field);
    ensure!(parts.len() == count, "invalid nmcli field count");
    for part in &parts {
        label(part)?;
    }
    Ok(parts)
}

pub fn wifi_scan(output: &str) -> Result<Vec<WifiNetwork>> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for (index, line) in output.lines().enumerate() {
        ensure!(index < 2048, "Wi-Fi list exceeds limit");
        let p = fields(line, 4)?;
        // nmcli prints "*" for the network in use and a space for every other one.
        ensure!(
            matches!(p[0].as_str(), "*" | " " | ""),
            "invalid active Wi-Fi flag"
        );
        let signal: u8 = p[2].parse().context("invalid Wi-Fi signal")?;
        ensure!(signal <= 100, "invalid Wi-Fi signal");
        if p[1].is_empty() {
            continue;
        } // hidden networks are not enrollment targets
        let in_use = p[0] == "*";
        if let Some(existing) = networks
            .iter_mut()
            .find(|n| n.ssid == p[1] && n.security == p[3])
        {
            existing.in_use |= in_use;
            existing.signal = existing.signal.max(signal);
            continue;
        }
        if networks.len() < 256 {
            networks.push(WifiNetwork {
                ssid: p[1].clone(),
                signal,
                security: p[3].clone(),
                in_use,
            });
        } else if in_use && let Some(pos) = networks.iter().rposition(|n| !n.in_use) {
            networks[pos] = WifiNetwork {
                ssid: p[1].clone(),
                signal,
                security: p[3].clone(),
                in_use,
            };
        }
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

/// A login password the helper may pass to passwd: not empty, under 512
/// bytes, and no control characters (a newline would end it early).
pub fn validate_login_password(password: &str) -> Result<()> {
    ensure!(
        !password.is_empty(),
        "type your current password and a new one"
    );
    ensure!(password.len() < 512, "passwords must be under 512 bytes");
    ensure!(
        !password.chars().any(char::is_control),
        "passwords cannot hold control characters"
    );
    Ok(())
}

/// wros-passwd's one JSON line: a change, or the reason it did not happen.
pub fn password_reply(output: &str) -> Result<crate::Reply> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Raw {
        changed: bool,
        message: String,
    }
    let raw: Raw = serde_json::from_str(output.trim()).context("invalid password helper reply")?;
    ensure!(
        !raw.message.is_empty()
            && raw.message.len() <= 300
            && !raw.message.chars().any(char::is_control),
        "invalid password helper reply"
    );
    if raw.changed {
        Ok(crate::Reply::Message(raw.message))
    } else {
        anyhow::bail!("{}", raw.message)
    }
}

pub fn validate_wifi_password(password: &str) -> Result<()> {
    ensure!(
        ((8..=63).contains(&password.len()) && !password.chars().any(char::is_control))
            || (password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit())),
        "Wi-Fi password must be 8–63 bytes, or an exact 64-digit hexadecimal WPA key"
    );
    Ok(())
}

pub fn parse_clock_reply(output: &str, expected: &str) -> Result<String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ClockReply {
        status: String,
        local_time: String,
        ntp: bool,
    }
    let reply: ClockReply =
        serde_json::from_str(output).context("invalid wros-clock reply JSON")?;
    ensure!(
        reply.status == "applied" && !reply.ntp,
        "local clock update was not confirmed with NTP disabled"
    );
    let normalized = crate::timezone::validate_local_datetime(&reply.local_time)?;
    ensure!(
        normalized == expected,
        "local clock readback did not match the requested time"
    );
    Ok(normalized)
}

/// The clock helper's reply after handing the clock back to network time.
pub fn parse_network_time_reply(output: &str) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NetworkTimeReply {
        status: String,
        ntp: bool,
    }
    let reply: NetworkTimeReply =
        serde_json::from_str(output).context("invalid wros-clock reply JSON")?;
    ensure!(
        reply.status == "applied" && reply.ntp,
        "network time was not confirmed on"
    );
    Ok(())
}

/// The sound helper's one JSON line (`/usr/libexec/wros-sound`), read strictly.
pub fn parse_sound_status(output: &str) -> Result<AudioStatus> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply {
        available: bool,
        volume_percent: Option<u8>,
        muted: bool,
        message: String,
    }
    ensure!(output.len() <= 4096, "oversized sound reply");
    let reply: Reply = serde_json::from_str(output.trim()).context("invalid sound reply")?;
    ensure!(
        reply.volume_percent.is_some_and(|v| v <= 100) == reply.available,
        "sound reply volume does not match its availability"
    );
    ensure!(
        reply.message.chars().count() <= 200,
        "sound reply message too long"
    );
    Ok(AudioStatus {
        available: reply.available,
        volume_percent: reply.volume_percent,
        muted: reply.muted,
        message: reply.message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sound_helper_reply_is_read_strictly() {
        let status = parse_sound_status(
            r#"{"available": true, "volume_percent": 69, "muted": true, "message": "Volume 69% · muted"}"#,
        )
        .unwrap();
        assert_eq!(
            (status.available, status.volume_percent, status.muted),
            (true, Some(69), true)
        );
        let none = parse_sound_status(
            r#"{"available": false, "volume_percent": null, "muted": false, "message": "No sound control: Could not find a sound card."}"#,
        )
        .unwrap();
        assert!(!none.available && none.volume_percent.is_none());
        for bad in [
            "",
            "Simple mixer control 'Master',0
  Mono: Playback 60 [69%] [on]",
            r#"{"available": true, "volume_percent": 101, "muted": false, "message": ""}"#,
            r#"{"available": true, "volume_percent": null, "muted": false, "message": ""}"#,
            r#"{"available": false, "volume_percent": 40, "muted": false, "message": ""}"#,
            r#"{"available": true, "volume_percent": 4, "muted": false, "message": "", "extra": 1}"#,
        ] {
            assert!(parse_sound_status(bad).is_err(), "{bad}");
        }
        let long = format!(
            r#"{{"available": true, "volume_percent": 4, "muted": false, "message": "{}"}}"#,
            "x".repeat(300)
        );
        assert!(parse_sound_status(&long).is_err());
    }
    #[test]
    fn wifi_preserves_spaces_and_decodes_escaped_names_without_guessing() {
        let rows = wifi_scan("*: Desk\\:Main\\\\West :91:WPA2\n:--:10:--").unwrap();
        assert_eq!(rows[0].ssid, " Desk:Main\\West ");
        assert!(rows[0].in_use);
        assert_eq!(rows[1].ssid, "--"); // this can be a real SSID
        let dedup = wifi_scan(":Campus:40:WPA2\n*:Campus:85:WPA2\n:Campus:60:WPA2").unwrap();
        assert_eq!(dedup.len(), 1);
        assert!(dedup[0].in_use);
        assert_eq!(dedup[0].signal, 85);
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
    fn wifi_reads_the_space_nmcli_prints_for_networks_not_in_use() {
        // Real `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list` output (names changed):
        // every network not in use starts with a space, so the whole scan used to fail with
        // "invalid active Wi-Fi flag" (#78). An open network has an empty SECURITY field.
        let rows =
            wifi_scan(" :Home:100:WPA2\n*:Home:87:WPA2\n :School:60:\n :Cafe:55:WPA2").unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].in_use && rows[0].ssid == "Home");
        assert_eq!(
            (
                rows[1].ssid.as_str(),
                rows[1].security.as_str(),
                rows[1].in_use
            ),
            ("School", "", false)
        );
        assert!(wifi_scan("x:Home:90:WPA2").is_err());
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
    }
}
