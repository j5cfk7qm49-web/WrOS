//! Cloud folder detection and experimental read-only local comparison models.
//!
//! These models perform no transfer, backup, restore or provider enrollment.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Client {
    Dropbox,
    GoogleDrive,
    PCloud,
    Box,
    OneDrive,
    ICloud,
}

impl Client {
    pub fn label(self) -> &'static str {
        match self {
            Client::Dropbox => "Dropbox",
            Client::GoogleDrive => "Google Drive",
            Client::PCloud => "pCloud",
            Client::Box => "Box",
            Client::OneDrive => "OneDrive",
            Client::ICloud => "iCloud Drive",
        }
    }
}

/// The sync service `root` sits inside, judged by the folders on the way to it
/// (each client's usual names, on Linux, macOS and Windows), the marker files
/// some leave at their top, and — on Linux — a pCloud Drive mount.
pub fn detect(root: &Path) -> Option<Client> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let home = crate::paths::home();
    by_names(&root, &home)
        .or_else(|| by_markers(&root))
        .or_else(|| by_mount(&root))
}

/// Folder names along the path. `home` decides the one ambiguous case: a
/// folder called just "Box" counts only straight under the home folder, where
/// Box Drive puts it.
fn by_names(root: &Path, home: &Path) -> Option<Client> {
    let parts: Vec<String> = root
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    for (i, part) in parts.iter().enumerate() {
        let p = part.as_str();
        let lower = p.to_lowercase();
        // macOS File Provider: ~/Library/CloudStorage/<Client>-<account>
        if i > 0 && parts[i - 1] == "CloudStorage" {
            let found = match lower.split(['-', ' ']).next().unwrap_or("") {
                "dropbox" => Some(Client::Dropbox),
                "googledrive" => Some(Client::GoogleDrive),
                "pcloud" => Some(Client::PCloud),
                "box" => Some(Client::Box),
                "onedrive" => Some(Client::OneDrive),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        if p == "Dropbox" || p.starts_with("Dropbox (") {
            return Some(Client::Dropbox);
        }
        if p == "Google Drive" || p == "My Drive" || p == "GoogleDrive" {
            return Some(Client::GoogleDrive);
        }
        if p == "pCloudDrive" || p == "pCloud Drive" || p == "pCloud Sync" {
            return Some(Client::PCloud);
        }
        if p == "Box Drive" || p == "Box Sync" || p == "Box-Box" {
            return Some(Client::Box);
        }
        if p == "Box" && root.starts_with(home.join("Box")) {
            return Some(Client::Box);
        }
        if p == "OneDrive" || p.starts_with("OneDrive - ") || p.starts_with("OneDrive-") {
            return Some(Client::OneDrive);
        }
        if p == "Mobile Documents" || p == "iCloud Drive" || p == "iCloudDrive" {
            return Some(Client::ICloud);
        }
    }
    None
}

/// Files a client leaves at the top of its folder.
fn by_markers(root: &Path) -> Option<Client> {
    let mut dir: Option<&Path> = Some(root);
    while let Some(d) = dir {
        if d.join(".dropbox").is_file() || d.join(".dropbox.cache").is_dir() {
            return Some(Client::Dropbox);
        }
        if d.join(".tmp.drivedownload").exists() || d.join(".tmp.driveupload").exists() {
            return Some(Client::GoogleDrive);
        }
        dir = d.parent();
    }
    None
}

/// Linux: the pCloud Drive FUSE mount (`fsname=pCloud.fs`) the book is under.
fn by_mount(root: &Path) -> Option<Client> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
    let mut best: Option<(usize, bool)> = None;
    for line in mounts.lines() {
        let mut f = line.split_whitespace();
        let (Some(source), Some(point)) = (f.next(), f.next()) else {
            continue;
        };
        let point = PathBuf::from(point.replace("\\040", " "));
        if root.starts_with(&point) {
            let len = point.as_os_str().len();
            if best.is_none_or(|(l, _)| len > l) {
                best = Some((len, source.contains("pCloud")));
            }
        }
    }
    best.and_then(|(_, pcloud)| pcloud.then_some(Client::PCloud))
}

// ── M9b: Conflict-Safe Bidirectional Sync Evaluation ─────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncDecision {
    UpToDate,
    PushLocal,
    PullRemote,
    ConflictParkBoth,
    TombstoneLocalDeleted,
    TombstoneRemoteDeleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncLedger {
    pub schema_version: u32,
    pub base_hashes: BTreeMap<String, String>,
}

impl Default for SyncLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncLedger {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            base_hashes: BTreeMap::new(),
        }
    }

    /// Compare local and remote SHA-256 hashes against the last synchronized
    /// base version without ever performing a destructive blind overwrite.
    pub fn evaluate_file(
        &self,
        relative_path: &str,
        local_sha256: Option<&str>,
        remote_sha256: Option<&str>,
        local_dirty: bool,
    ) -> SyncDecision {
        if self.schema_version != 1 || local_dirty {
            return SyncDecision::ConflictParkBoth;
        }
        let base = self.base_hashes.get(relative_path).map(String::as_str);
        match (base, local_sha256, remote_sha256) {
            (_, None, None) => SyncDecision::UpToDate,
            (_, Some(l), Some(r)) if l == r => SyncDecision::UpToDate,
            (Some(b), Some(l), Some(r)) if l == b && r != b => SyncDecision::PullRemote,
            (Some(b), Some(l), Some(r)) if r == b && l != b => SyncDecision::PushLocal,
            (None, Some(_), None) => SyncDecision::PushLocal,
            (None, None, Some(_)) => SyncDecision::PullRemote,
            (Some(b), None, Some(r)) if r == b => SyncDecision::TombstoneLocalDeleted,
            (Some(b), Some(l), None) if l == b => SyncDecision::TombstoneRemoteDeleted,
            _ => SyncDecision::ConflictParkBoth,
        }
    }
}

// ── M9b: Google Docs Round-Trip Interoperability Evaluator ───────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsRoundTripAudit {
    pub lossless: bool,
    pub preserves_yaml_frontmatter: bool,
    pub preserves_private_notes: bool,
    pub preserves_footnotes: bool,
    pub preserves_inline_markdown: bool,
    pub differences: Vec<String>,
    pub recommended_workflow: String,
}

/// Evaluate whether a round-tripped text (e.g. exported to Google Docs and
/// converted back to Markdown/text) faithfully preserves all manuscript
/// structures, or whether it must be imported only as a separate experimental
/// review scene (`docs/SYNC_AND_BACKUP.md`).
pub fn evaluate_google_docs_roundtrip(original_md: &str, returned_md: &str) -> DocsRoundTripAudit {
    let mut differences = Vec::new();
    if original_md != returned_md {
        differences.push(
            "Original and returned source bytes differ; canonical replacement is not authorized."
                .into(),
        );
    }

    let has_yaml = original_md.starts_with("---\n");
    let yaml_preserved = if has_yaml {
        let orig_front = original_md.split("\n---\n").next().unwrap_or("");
        returned_md.starts_with("---\n")
            && returned_md.split("\n---\n").next().unwrap_or("") == orig_front
    } else {
        true
    };
    if !yaml_preserved {
        differences
            .push("YAML frontmatter or custom metadata fields were altered or stripped.".into());
    }

    let orig_notes = crate::notes::note_ranges(original_md);
    let ret_notes = crate::notes::note_ranges(returned_md);
    let orig_note_texts: Vec<&str> = orig_notes
        .iter()
        .map(|&(a, b)| &original_md[a..b])
        .collect();
    let ret_note_texts: Vec<&str> = ret_notes.iter().map(|&(a, b)| &returned_md[a..b]).collect();
    let notes_preserved = orig_note_texts == ret_note_texts;
    if !notes_preserved {
        differences.push("Private inline notes (%% ... %%) were stripped or modified.".into());
    }

    let orig_fn = crate::footnotes::analyze_footnotes(original_md);
    let ret_fn = crate::footnotes::analyze_footnotes(returned_md);
    let fn_preserved = orig_fn
        .references
        .iter()
        .map(|r| &r.label)
        .eq(ret_fn.references.iter().map(|r| &r.label))
        && orig_fn.definitions == ret_fn.definitions;
    if !fn_preserved {
        differences.push(
            "Contextual Markdown footnotes ([^label]) were altered or converted to plain endnotes."
                .into(),
        );
    }

    // Marker counts cannot establish preserved emphasis or its position.
    // Conservatively claim preservation only for an exact source match.
    let inline_preserved = original_md == returned_md;
    if !inline_preserved {
        differences
            .push("Inline Markdown emphasis markers (* or _) changed during conversion.".into());
    }

    let lossless = original_md == returned_md
        && yaml_preserved
        && notes_preserved
        && fn_preserved
        && inline_preserved;

    let recommended_workflow = if lossless {
        "Exact local byte match; provider round-trip behavior has not been qualified.".into()
    } else {
        "Information loss detected: do NOT overwrite canonical manuscript scenes. Import as a separate review/experimental scene or use one-way DOCX review copies.".into()
    };

    DocsRoundTripAudit {
        lossless,
        preserves_yaml_frontmatter: yaml_preserved,
        preserves_private_notes: notes_preserved,
        preserves_footnotes: fn_preserved,
        preserves_inline_markdown: inline_preserved,
        differences,
        recommended_workflow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(p: &str) -> Option<Client> {
        by_names(Path::new(p), Path::new("/home/ann"))
    }

    #[test]
    fn each_client_is_known_by_its_folders() {
        assert_eq!(at("/home/ann/Dropbox/Novels/Salt"), Some(Client::Dropbox));
        assert_eq!(at("/home/ann/Dropbox (Acme)/Salt"), Some(Client::Dropbox));
        assert_eq!(
            at("/Users/ann/Library/CloudStorage/Dropbox/Salt"),
            Some(Client::Dropbox)
        );
        assert_eq!(
            at("/Users/ann/Library/CloudStorage/GoogleDrive-ann@example.com/My Drive/Salt"),
            Some(Client::GoogleDrive)
        );
        assert_eq!(at("G:/My Drive/Salt"), Some(Client::GoogleDrive));
        assert_eq!(at("/home/ann/pCloudDrive/Salt"), Some(Client::PCloud));
        assert_eq!(at("/Users/ann/pCloud Drive/Salt"), Some(Client::PCloud));
        assert_eq!(
            at("/Users/ann/Library/CloudStorage/Box-Box/Salt"),
            Some(Client::Box)
        );
        assert_eq!(at("/home/ann/Box/Salt"), Some(Client::Box));
        assert_eq!(
            at("/home/ann/Documents/Box/Salt"),
            None,
            "a box is just a box"
        );
        assert_eq!(at("/home/ann/OneDrive - Acme/Salt"), Some(Client::OneDrive));
        assert_eq!(
            at("/Users/ann/Library/CloudStorage/OneDrive-Personal/Salt"),
            Some(Client::OneDrive)
        );
        assert_eq!(
            at("/Users/ann/Library/Mobile Documents/com~apple~CloudDocs/Salt"),
            Some(Client::ICloud)
        );
        assert_eq!(at("C:/Users/ann/iCloudDrive/Salt"), Some(Client::ICloud));
        assert_eq!(at("/home/ann/Documents/Grimoire"), None);
    }

    #[test]
    fn a_dropbox_marker_is_enough() {
        let d = std::env::temp_dir().join(format!("grimoire-cloud-{}", std::process::id()));
        let book = d.join("Novels/Salt");
        std::fs::create_dir_all(&book).unwrap();
        assert_eq!(by_markers(&book), None);
        std::fs::write(d.join(".dropbox"), "{}").unwrap();
        assert_eq!(by_markers(&book), Some(Client::Dropbox));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn sync_ledger_distinguishes_clean_updates_from_conflicts_and_dirty_buffers() {
        let mut ledger = SyncLedger::new();
        ledger
            .base_hashes
            .insert("scene.md".into(), "hash-v1".into());

        assert_eq!(
            ledger.evaluate_file("scene.md", Some("hash-v1"), Some("hash-v1"), false),
            SyncDecision::UpToDate
        );
        assert_eq!(
            ledger.evaluate_file("scene.md", Some("hash-v1"), Some("hash-v2"), false),
            SyncDecision::PullRemote
        );
        assert_eq!(
            ledger.evaluate_file("scene.md", Some("hash-v2"), Some("hash-v1"), false),
            SyncDecision::PushLocal
        );
        assert_eq!(
            ledger.evaluate_file("scene.md", Some("hash-v2"), Some("hash-v3"), false),
            SyncDecision::ConflictParkBoth
        );
        assert_eq!(
            ledger.evaluate_file("scene.md", Some("hash-v1"), Some("hash-v2"), true),
            SyncDecision::ConflictParkBoth,
            "never overwrite a dirty buffer"
        );
    }

    #[test]
    fn google_docs_roundtrip_auditor_flags_lost_yaml_footnotes_and_private_notes() {
        let original = "---\ntitle: \"The Brass Key\"\ncustom_field: \"λ\"\n---\n\nShe waited[^1] in the *dark*. %% private note %%\n\n[^1]: Original footnote.\n";
        let exact = evaluate_google_docs_roundtrip(original, original);
        assert!(exact.lossless);

        let degraded = "She waited[1] in the dark.\n\n[1] Original footnote.\n";
        let audit = evaluate_google_docs_roundtrip(original, degraded);
        assert!(!audit.lossless);
        assert!(!audit.preserves_yaml_frontmatter);
        assert!(!audit.preserves_private_notes);
        assert!(!audit.preserves_footnotes);
        assert!(!audit.preserves_inline_markdown);
        assert!(
            audit
                .recommended_workflow
                .contains("Information loss detected")
        );
    }
}
