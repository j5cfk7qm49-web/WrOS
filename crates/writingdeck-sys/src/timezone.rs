//! Bounded timezone identifiers from the local systemd/tzdata catalog.
use anyhow::{Result, bail};

pub fn validate(zone: &str) -> Result<()> {
    if zone.is_empty()
        || zone.len() > 128
        || !zone.as_bytes()[0].is_ascii_alphabetic()
        || zone.split('/').any(str::is_empty)
        || !zone
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/_+-".contains(&c))
    {
        bail!("invalid timezone identifier");
    }
    Ok(())
}

pub fn catalog(output: &str) -> Result<Vec<String>> {
    if output.len() >= 65_536 {
        bail!("timezone catalog exceeds limit");
    }
    let mut zones = Vec::new();
    for line in output.lines() {
        validate(line)?;
        if zones.len() >= 2048 {
            bail!("timezone catalog exceeds limit");
        }
        zones.push(line.to_string());
    }
    zones.sort();
    zones.dedup();
    if zones.is_empty() {
        bail!("timezone catalog is unavailable");
    }
    Ok(zones)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_local_catalog_names_and_refuses_paths_options_and_controls() {
        for zone in [
            "UTC",
            "Etc/GMT+8",
            "America/Argentina/Buenos_Aires",
            "Europe/London",
        ] {
            validate(zone).unwrap();
        }
        for zone in [
            "",
            "--host=other",
            "/etc/passwd",
            "../UTC",
            "Etc//UTC",
            "Etc/",
            "UTC\n",
            "UTC;reboot",
            "Åsia/Tokyo",
        ] {
            assert!(validate(zone).is_err(), "{zone:?}");
        }
        assert!(validate(&"A".repeat(129)).is_err());
    }
    #[test]
    fn entire_catalog_is_validated_before_it_becomes_selectable() {
        assert_eq!(
            catalog("UTC\nEurope/London\nUTC").unwrap(),
            ["Europe/London", "UTC"]
        );
        assert!(catalog("UTC\ninvalid timezone").is_err());
        assert!(catalog("").is_err());
        assert!(catalog(&"UTC\n".repeat(2049)).is_err());
        assert!(catalog(&"A".repeat(65_536)).is_err());
    }
}
