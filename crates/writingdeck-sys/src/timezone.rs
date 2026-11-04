//! Bounded timezone identifiers from the local systemd/tzdata catalog, plus
//! strict local-clock timestamp validation (offline-first, NTP off).
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

/// Validate a manual local timestamp (`YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DD HH:MM`)
/// and normalize it to `YYYY-MM-DD HH:MM:SS`. Rejects control characters, shell
/// metacharacters, invalid leap-year dates, and years outside `2020..=2099`.
pub fn validate_local_datetime(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.len() > 32 || trimmed.chars().any(char::is_control) {
        bail!("invalid local clock timestamp");
    }
    let (date_part, time_part) = trimmed
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("enter local date and time as YYYY-MM-DD HH:MM:SS"))?;
    let date_fields: Vec<&str> = date_part.split('-').collect();
    if date_fields.len() != 3
        || date_fields[0].len() != 4
        || date_fields[1].len() != 2
        || date_fields[2].len() != 2
        || !date_fields
            .iter()
            .all(|f| f.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("invalid date; expected YYYY-MM-DD");
    }
    let year: u32 = date_fields[0].parse()?;
    let month: u32 = date_fields[1].parse()?;
    let day: u32 = date_fields[2].parse()?;
    if !(2020..=2099).contains(&year) || !(1..=12).contains(&month) {
        bail!("date out of supported range (2020..2099)");
    }
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => unreachable!(),
    };
    if !(1..=max_day).contains(&day) {
        bail!("invalid calendar day for month");
    }
    let time_fields: Vec<&str> = time_part.split(':').collect();
    if !(2..=3).contains(&time_fields.len())
        || !time_fields
            .iter()
            .all(|f| f.len() == 2 && f.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("invalid time; expected HH:MM or HH:MM:SS");
    }
    let hour: u32 = time_fields[0].parse()?;
    let minute: u32 = time_fields[1].parse()?;
    let second: u32 = if time_fields.len() == 3 {
        time_fields[2].parse()?
    } else {
        0
    };
    if hour > 23 || minute > 59 || second > 59 {
        bail!("time out of range (00:00:00..23:59:59)");
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
    ))
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
    #[test]
    fn local_datetime_validator_normalizes_and_rejects_invalid_dates_or_flags() {
        assert_eq!(
            validate_local_datetime("2026-10-31 19:45").unwrap(),
            "2026-10-31 19:45:00"
        );
        assert_eq!(
            validate_local_datetime("2024-02-29 23:59:59").unwrap(),
            "2024-02-29 23:59:59"
        );
        for bad in [
            "",
            "2026-02-29 12:00:00",
            "2019-12-31 23:59:59",
            "2100-01-01 00:00:00",
            "2026-13-01 00:00:00",
            "2026-10-31 24:00:00",
            "2026-10-31 12:60:00",
            "2026-10-31 12:00:00;reboot",
            "--set-ntp=true",
        ] {
            assert!(validate_local_datetime(bad).is_err(), "{bad:?}");
        }
    }
}
