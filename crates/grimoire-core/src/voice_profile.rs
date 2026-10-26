//! Author-selected descriptive reference profiles, not authorship detectors.
//! Store numeric aggregates and immutable source references only. A baseline is
//! a frozen snapshot, never a second copy or a live rewrite of the manuscript.
use crate::{
    diagnostics::{Snapshot, SourceRole, SourceUnit},
    prose::{self, Metrics, Options, Status},
    statefile::{Store, Validate},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};
pub const RELATIVE: &str = ".grimoire/writingdeck/voice-profiles.json";
pub const ALGORITHM: &str = "writingdeck-native-1";
pub const MIN_WORDS: u64 = 5_000;
pub const MIN_SEGMENTS: u64 = 40;
pub const MIN_DISTINCT_WORDS: u64 = 150;
pub const MIN_COMPARE_WORDS: u64 = 300;
pub const MAX_REFERENCES: usize = 64;
pub const MAX_PROFILES: usize = 8;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub document: String,
    pub unit: SourceUnit,
    pub role: SourceRole,
    pub sha256: String,
    pub analysis_words: u64,
    pub unique_normalized_words: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceProfile {
    pub id: u64,
    pub label: String,
    pub algorithm: String,
    pub created_at: String,
    pub options: Options,
    pub references: Vec<Reference>,
    pub metrics: Metrics,
}
impl VoiceProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.id > 0, "invalid voice profile id");
        label(&self.label)?;
        ensure!(
            self.algorithm == ALGORITHM,
            "unknown voice analysis algorithm; original preserved"
        );
        ensure!(
            self.created_at.len() <= 64
                && chrono::DateTime::parse_from_rfc3339(&self.created_at).is_ok(),
            "invalid voice reference timestamp"
        );
        self.options.analysis.validate()?;
        self.metrics.validate()?;
        ensure!(
            !self.references.is_empty() && self.references.len() <= MAX_REFERENCES,
            "invalid voice reference count"
        );
        let mut documents = BTreeSet::new();
        let mut hashes = BTreeSet::new();
        let mut words = 0;
        for reference in &self.references {
            ensure!(
                reference.document.len() <= 4096
                    && !reference.document.is_empty()
                    && !reference.document.chars().any(char::is_control)
                    && Path::new(&reference.document)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "voice reference must remain project-relative"
            );
            ensure!(
                reference.role == SourceRole::Manuscript,
                "research/notes are not author-reference manuscript material"
            );
            ensure!(
                hash(&reference.sha256)
                    && hashes.insert(&reference.sha256)
                    && documents.insert(&reference.document),
                "duplicate or invalid reference; do not inflate material by replaying a source"
            );
            ensure!(
                (1..=65_536).contains(&reference.analysis_words)
                    && reference.unique_normalized_words <= reference.analysis_words,
                "invalid reference material count"
            );
            words += reference.analysis_words;
        }
        ensure!(
            words == self.metrics.analysis_words
                && words >= MIN_WORDS
                && self.metrics.sentence_segments >= MIN_SEGMENTS
                && self
                    .references
                    .iter()
                    .any(|r| r.unique_normalized_words >= MIN_DISTINCT_WORDS),
            "insufficient reference material: need 5,000 analysis words, 40 segments and at least one source with 150 distinct normalized words"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub schema_version: u32,
    pub next_id: u64,
    pub active: Option<u64>,
    pub profiles: Vec<VoiceProfile>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: 1,
            next_id: 1,
            active: None,
            profiles: vec![],
        }
    }
}
impl Validate for Library {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unknown/future voice profile schema; original preserved"
        );
        ensure!(
            self.next_id > 0 && self.profiles.len() <= MAX_PROFILES,
            "invalid voice library limits"
        );
        let mut ids = BTreeSet::new();
        let mut labels = BTreeSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            ensure!(
                ids.insert(profile.id)
                    && profile.id < self.next_id
                    && labels.insert(&profile.label),
                "duplicate/replayed voice profile id or label"
            );
        }
        ensure!(
            self.active.is_none_or(|id| ids.contains(&id)),
            "active voice reference is missing"
        );
        Ok(())
    }
}
pub fn load(root: &Path) -> Result<Store<Library>> {
    Store::load(root, Path::new(RELATIVE), Library::default())
}
/// Prepared in a bounded background job from explicitly selected originals. No
/// disk mutation or profile id allocation occurs until the explicit save action.
#[derive(Clone)]
pub struct Prepared {
    label: String,
    created_at: String,
    options: Options,
    references: Vec<Reference>,
    metrics: Metrics,
}
impl Prepared {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn references(&self) -> &[Reference] {
        &self.references
    }
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}
pub fn prepare(
    name: &str,
    sources: &[Snapshot],
    options: Options,
    created_at: &str,
) -> Result<Prepared> {
    label(name)?;
    ensure!(
        !sources.is_empty() && sources.len() <= MAX_REFERENCES,
        "select one to 64 original manuscript sources"
    );
    let mut references = vec![];
    let mut metrics = vec![];
    for source in sources {
        ensure!(
            source.stamp.role == SourceRole::Manuscript,
            "exclude research, notes and third-party references from author baseline"
        );
        let report = prose::analyze(source.clone(), options.clone())?;
        ensure!(
            report.status == Status::Complete,
            "reference is empty/too large; no partial baseline is created"
        );
        references.push(Reference {
            document: source.stamp.document.clone(),
            unit: source.stamp.unit,
            role: source.stamp.role,
            sha256: source.stamp.sha256.clone(),
            analysis_words: report.metrics.analysis_words,
            unique_normalized_words: report.metrics.unique_normalized_words,
        });
        metrics.push(report.metrics);
    }
    references.sort_by(|a, b| a.document.cmp(&b.document));
    let metrics = prose::merge_metrics(&metrics)?;
    let candidate = VoiceProfile {
        id: 1,
        label: name.into(),
        algorithm: ALGORITHM.into(),
        created_at: created_at.into(),
        options: options.clone(),
        references: references.clone(),
        metrics: metrics.clone(),
    };
    candidate.validate()?;
    Ok(Prepared {
        label: name.into(),
        created_at: created_at.into(),
        options,
        references,
        metrics,
    })
}
pub fn save(store: &mut Store<Library>, prepared: Prepared) -> Result<u64> {
    store.update(move|library| {
        if let Some(old)=library.profiles.iter().find(|p|p.label==prepared.label) {
            ensure!(old.references==prepared.references && old.options.digest()==prepared.options.digest() && old.metrics==prepared.metrics,"existing voice label refers to different material; choose a new label, original preserved");
            library.active=Some(old.id);return Ok(old.id)
        }
        ensure!(library.profiles.len()<MAX_PROFILES,"eight reference profiles retained; explicitly remove an unused profile first");
        let id=library.next_id;library.next_id=id.checked_add(1).ok_or_else(||anyhow::anyhow!("voice id space exhausted"))?;
        library.profiles.push(VoiceProfile{id,label:prepared.label,algorithm:ALGORITHM.into(),created_at:prepared.created_at,options:prepared.options,references:prepared.references,metrics:prepared.metrics});
        library.active=Some(id);Ok(id)
    })
}
pub fn activate(store: &mut Store<Library>, id: u64) -> Result<()> {
    store.update(|library| {
        ensure!(
            library.profiles.iter().any(|p| p.id == id),
            "voice profile no longer exists"
        );
        library.active = Some(id);
        Ok(())
    })
}
pub fn remove(store: &mut Store<Library>, id: u64) -> Result<()> {
    store.update(|library| {
        ensure!(
            library.profiles.iter().any(|p| p.id == id),
            "voice profile no longer exists"
        );
        library.profiles.retain(|p| p.id != id);
        if library.active == Some(id) {
            library.active = None;
        }
        Ok(())
    })
}
#[derive(Debug, Clone, Serialize)]
pub struct Deviation {
    pub feature: String,
    pub reference: f64,
    pub current: f64,
    pub relative_change: f64,
    pub explanation: String,
}
pub fn compare(profile: &VoiceProfile, current: &prose::Report) -> Result<Vec<Deviation>> {
    profile.validate()?;
    ensure!(
        current.status == Status::Complete,
        "current analysis is unavailable/incomplete"
    );
    current.metrics.validate()?;
    ensure!(
        current.snapshot.stamp.role == SourceRole::Manuscript,
        "voice comparison requires manuscript prose"
    );
    ensure!(
        current.metrics.analysis_words >= MIN_COMPARE_WORDS
            && current.metrics.sentence_segments >= 5,
        "need at least 300 current analysis words and five segments; no reliable deviation is claimed for a fragment"
    );
    ensure!(
        profile.options.analysis.include_footnotes == current.options.analysis.include_footnotes,
        "reference/current footnote scope differs; select a matching profile"
    );
    let baseline = profile.metrics.voice_features();
    let now = current.metrics.voice_features();
    let mut out = vec![];
    for (feature, reference) in baseline {
        let Some(current) = now.get(&feature).copied() else {
            continue;
        };
        let floor = match feature.as_str() {
            "sentence_mean" => 5.0,
            "sentence_sd" => 3.0,
            "paragraph_mean" => 30.0,
            "word_letters_mean" => 0.5,
            "dialogue_fraction_heuristic" => 0.15,
            "long_word_fraction" => 0.1,
            _ if feature.starts_with("function_") => 5.0,
            _ => 3.0,
        };
        let relative_change = (current - reference) / reference.abs().max(floor);
        if (current - reference).abs() >= floor && relative_change.abs() >= 0.30 {
            out.push(Deviation{explanation:format!("{} than the selected reference. Narrator, scene purpose, form and genre may explain this; it is not an error or authorship evidence.",if current>reference{"Higher"}else{"Lower"}),feature,reference,current,relative_change});
        }
    }
    // Information order by magnitude of this feature's change, not an overall
    // literary score, ranking of writers, or a claim of human authorship.
    out.sort_by(|a, b| {
        b.relative_change
            .abs()
            .total_cmp(&a.relative_change.abs())
            .then_with(|| a.feature.cmp(&b.feature))
    });
    Ok(out)
}
pub fn reference_matches(profile: &VoiceProfile, sources: &[Snapshot]) -> bool {
    profile.references.iter().all(|reference| {
        sources.iter().any(|s| {
            s.stamp.document == reference.document
                && s.stamp.unit == reference.unit
                && s.stamp.role == reference.role
                && s.stamp.sha256 == reference.sha256
        })
    })
}
fn label(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty()
            && name == name.trim()
            && name.chars().count() <= 64
            && !name.chars().any(char::is_control),
        "voice reference label must be 1–64 plain characters"
    );
    Ok(())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    fn root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "wd-voice-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn source(path: &str, text: &str) -> Snapshot {
        Snapshot::new(
            Path::new(path),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            1,
            text,
        )
        .unwrap()
    }
    // Deliberately synthetic numeric-threshold test material, NOT evidence of
    // anyone's authorship, real voice representativeness or literary quality.
    fn material() -> String {
        (0..40)
            .map(|i| {
                format!(
                    "{}.",
                    (0..130)
                        .map(|j| format!("lexeme{}", (j + i) % 160))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    fn prepared() -> Prepared {
        prepare(
            "Reference",
            &[source("scene.md", &material())],
            Options::default(),
            "2026-10-26T10:00:00Z",
        )
        .unwrap()
    }
    #[test]
    fn short_repeated_empty_and_research_material_cannot_become_a_fake_voice_profile() {
        assert!(
            prepare(
                "Reference",
                &[source("short.md", "A short paragraph.")],
                Options::default(),
                "2026-10-26T10:00:00Z"
            )
            .is_err()
        );
        let repeated = ("word ".repeat(130) + ".\n\n").repeat(40);
        assert!(
            prepare(
                "Reference",
                &[source("repeat.md", &repeated)],
                Options::default(),
                "2026-10-26T10:00:00Z"
            )
            .is_err()
        );
        let note = Snapshot::new(
            Path::new("research.md"),
            SourceUnit::EditorBody,
            SourceRole::Research,
            1,
            material(),
        )
        .unwrap();
        assert!(
            prepare(
                "Reference",
                &[note],
                Options::default(),
                "2026-10-26T10:00:00Z"
            )
            .is_err()
        );
    }
    #[test]
    fn exact_sources_are_not_counted_twice_and_source_snapshots_are_frozen() {
        let a = source("scene.md", &material());
        let b = source("copy.md", a.text());
        assert!(
            prepare(
                "Reference",
                &[a.clone(), b],
                Options::default(),
                "2026-10-26T10:00:00Z"
            )
            .is_err()
        );
        let root = root();
        let mut store = load(&root).unwrap();
        save(&mut store, prepared()).unwrap();
        let profile = &store.value.profiles[0];
        assert!(reference_matches(profile, &[a]));
        assert!(!reference_matches(
            profile,
            &[source("scene.md", "later revision")]
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn save_is_explicit_idempotent_and_stores_references_and_numbers_not_manuscript_text() {
        let root = root();
        let mut store = load(&root).unwrap();
        assert!(!root.join(RELATIVE).exists());
        let id = save(&mut store, prepared()).unwrap();
        assert_eq!(save(&mut store, prepared()).unwrap(), id);
        assert_eq!(store.value.profiles.len(), 1);
        assert_eq!(store.value.next_id, 2);
        let bytes = fs::read_to_string(root.join(RELATIVE)).unwrap();
        assert!(!bytes.contains("lexeme") && !bytes.contains("original_text"));
        assert!(bytes.contains("sha256"));
        assert_eq!(load(&root).unwrap().value.active, Some(id));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn future_corrupt_external_and_conflicting_label_state_is_preserved() {
        let root = root();
        let mut store = load(&root).unwrap();
        save(&mut store, prepared()).unwrap();
        let path = root.join(RELATIVE);
        let mut conflicting = prepared();
        conflicting.references[0].document = "elsewhere.md".into();
        let before = fs::read(&path).unwrap();
        assert!(save(&mut store, conflicting).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        for bytes in [b"{\"schema_version\":99}".as_slice(), b"malformed"] {
            fs::write(&path, bytes).unwrap();
            assert!(load(&root).is_err());
            assert!(save(&mut store, prepared()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn comparison_requires_material_and_shows_feature_differences_not_a_detector_or_quality_score()
    {
        let root = root();
        let mut store = load(&root).unwrap();
        save(&mut store, prepared()).unwrap();
        let profile = &store.value.profiles[0];
        let short = prose::analyze(source("current.md", "I waited."), Options::default()).unwrap();
        assert!(compare(profile, &short).is_err());
        let now = prose::analyze(
            source(
                "current.md",
                &"I saw the sea. We waited for her.\n\n".repeat(60),
            ),
            Options::default(),
        )
        .unwrap();
        let differences = compare(profile, &now).unwrap();
        assert!(!differences.is_empty());
        assert!(differences.iter().all(|d| {
            d.explanation
                .contains("not an error or authorship evidence")
        }));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn deletion_does_not_replay_ids_or_delete_sources_and_invalid_numbers_are_rejected() {
        let root = root();
        let mut store = load(&root).unwrap();
        let id = save(&mut store, prepared()).unwrap();
        remove(&mut store, id).unwrap();
        assert_eq!(store.value.active, None);
        assert_eq!(save(&mut store, prepared()).unwrap(), 2);
        let mut metrics = store.value.profiles[0].metrics.clone();
        metrics.word_letters = u64::MAX;
        assert!(metrics.validate().is_err());
        metrics = store.value.profiles[0].metrics.clone();
        metrics.approximate_english_grade = Some(f64::NAN);
        assert!(metrics.validate().is_err());
        let _ = fs::remove_dir_all(root);
    }
}
