//! Bounded read-only views of maintained Debian services. None of these
//! parsers infer successful authentication, backup, update or recovery.
use crate::{LocalAccount, SavedWifi, StorageDevice, UpdateInfo, WifiNetwork};
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
}
