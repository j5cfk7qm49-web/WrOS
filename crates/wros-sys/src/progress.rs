//! Online update progress and timing for the system activity line.
use serde::Deserialize;

/// How far the update engine's download got (#167), from the progress file it keeps in the
/// root-owned online cache while it downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchProgress {
    pub verifying: bool,
    pub version: String,
    pub done: u64,
    pub total: u64,
}

pub const FETCH_PROGRESS_PATH: &str = "/var/cache/wros-update/online/progress.json";

impl FetchProgress {
    #[must_use]
    pub fn read() -> Option<Self> {
        let file = std::fs::File::open(FETCH_PROGRESS_PATH).ok()?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::Read::take(file, 4096), &mut text).ok()?;
        Self::parse(&text)
    }

    /// Untrusted numbers: anything odd and the plain busy line is shown instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Record {
            step: String,
            version: String,
            done: u64,
            total: u64,
        }
        let record: Record = serde_json::from_str(text).ok()?;
        let verifying = match record.step.as_str() {
            "download" => false,
            "verify" => true,
            _ => return None,
        };
        let version_ok = !record.version.is_empty()
            && record.version.len() <= 40
            && record
                .version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".~-+_".contains(c));
        (version_ok && record.done <= record.total).then_some(Self {
            verifying,
            version: record.version,
            done: record.done,
            total: record.total,
        })
    }

    /// "Downloading WrOS 0.1.0~rc20: 34 of 72 MB" or "Checking the downloaded WrOS 0.1.0~rc20…".
    pub fn line(&self) -> String {
        if self.verifying {
            return format!(
                "Checking the signatures of the downloaded WrOS {}…",
                self.version
            );
        }
        const MB: u64 = 1_000_000;
        format!(
            "Downloading WrOS {}: {} of {} MB",
            self.version,
            self.done / MB,
            self.total.div_ceil(MB)
        )
    }
}

/// How long something has run, for a busy line: nothing for the first two seconds
/// (most requests are done by then), then "0:07", "1:42".
#[must_use]
pub fn elapsed_label(elapsed: std::time::Duration) -> Option<String> {
    let secs = elapsed.as_secs();
    (secs >= 2).then(|| format!("{}:{:02}", secs / 60, secs % 60))
}

#[cfg(test)]
mod progress_tests {
    use super::{FetchProgress, elapsed_label};
    use crate::Request;
    use std::time::Duration;

    #[test]
    fn fetch_progress_is_read_as_untrusted_numbers() {
        let good = FetchProgress::parse(
            r#"{"step":"download","version":"0.1.0~rc20","done":1500000,"total":72400000}"#,
        )
        .unwrap();
        assert_eq!(good.line(), "Downloading WrOS 0.1.0~rc20: 1 of 73 MB");
        let verify =
            FetchProgress::parse(r#"{"step":"verify","version":"0.1.0~rc20","done":5,"total":5}"#)
                .unwrap();
        assert!(verify.line().starts_with("Checking the signatures"));
        for bad in [
            r#"{"step":"download","version":"0.1.0~rc20","done":9,"total":5}"#,
            r#"{"step":"other","version":"0.1.0~rc20","done":1,"total":5}"#,
            r#"{"step":"download","version":"\u001b[2J","done":1,"total":5}"#,
            r#"{"step":"download","version":"","done":1,"total":5}"#,
            r#"{"step":"download","version":"1","done":-1,"total":5}"#,
            "not json",
        ] {
            assert_eq!(FetchProgress::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn busy_lines_name_the_request_and_count_once_it_takes_a_while() {
        assert_eq!(
            Request::FetchOnlineUpdate.activity(),
            "Downloading the WrOS update…"
        );
        assert_eq!(elapsed_label(Duration::from_millis(1900)), None);
        assert_eq!(
            elapsed_label(Duration::from_secs(7)).as_deref(),
            Some("0:07")
        );
        assert_eq!(
            elapsed_label(Duration::from_secs(102)).as_deref(),
            Some("1:42")
        );
    }
}
