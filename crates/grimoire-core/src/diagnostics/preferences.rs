//! Guarded, reference/settings-only local and personal diagnostic preferences.
//! Reading defaults never creates directories or opts a writer into analysis.
use super::{AnalysisOptions, RuleKey};
use crate::statefile::{Store, Validate};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const NOVEL_FILE: &str = ".grimoire/writingdeck/checks.json";
/// `personal_root` is the writer's home/profile, not a manuscript project.
/// A distinct basename avoids a collision when home itself is opened as a book.
pub const GLOBAL_FILE: &str = ".grimoire/writingdeck/checks-global.json";
/// Changing the engine/model/runtime contract requires a fresh explicit opt-in.
pub const CONSENT_VERSION: &str = "harper-core-2.13.0/brill-2.13.0/pos-2.13.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovelPreferences {
    pub version: u32,
    pub live_consent: Option<String>,
    pub options: AnalysisOptions,
}
impl Default for NovelPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            live_consent: None,
            options: AnalysisOptions::default(),
        }
    }
}
impl Validate for NovelPreferences {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported diagnostic settings version; original preserved"
        );
        if let Some(consent) = &self.live_consent {
            ensure!(
                consent.len() <= 128 && !consent.chars().any(char::is_control),
                "invalid diagnostic consent record"
            );
        }
        ensure!(
            self.options.disabled_rules.len() <= 256,
            "too many novel muted rules"
        );
        self.options.validate()
    }
}
impl NovelPreferences {
    pub fn live_enabled(&self) -> bool {
        self.live_consent.as_deref() == Some(CONSENT_VERSION)
    }
    pub fn load(root: &Path) -> Result<Store<Self>> {
        Store::load(root, Path::new(NOVEL_FILE), Self::default())
    }
    pub fn effective(&self, personal: &PersonalPreferences) -> AnalysisOptions {
        let mut options = self.options.clone();
        options
            .disabled_rules
            .extend(personal.muted_rules.iter().cloned());
        options
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalPreferences {
    pub version: u32,
    pub allowed_runtime: Option<String>,
    /// Canonical project-root/runtime hashes, never paths or prose.
    pub live_projects: BTreeSet<String>,
    pub muted_rules: BTreeSet<RuleKey>,
}
impl Default for PersonalPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            allowed_runtime: None,
            live_projects: BTreeSet::new(),
            muted_rules: BTreeSet::new(),
        }
    }
}
impl Validate for PersonalPreferences {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported personal diagnostic settings version; original preserved"
        );
        ensure!(
            self.muted_rules.len() <= 256,
            "too many personal muted rules"
        );
        if let Some(runtime) = &self.allowed_runtime {
            ensure!(
                runtime.len() <= 128 && !runtime.chars().any(char::is_control),
                "invalid personal diagnostic consent record"
            );
        }
        ensure!(
            self.live_projects.len() <= 256,
            "too many live project permissions; revoke unused permissions explicitly"
        );
        for binding in &self.live_projects {
            ensure!(
                binding.len() == 64
                    && binding
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                "invalid live project binding"
            );
        }
        for rule in &self.muted_rules {
            rule.validate()?;
        }
        Ok(())
    }
}
impl PersonalPreferences {
    pub fn allows_runtime(&self) -> bool {
        self.allowed_runtime.as_deref() == Some(CONSENT_VERSION)
    }
    pub fn load(personal_root: &Path) -> Result<Store<Self>> {
        Store::load(personal_root, Path::new(GLOBAL_FILE), Self::default())
    }
}

/// A copied novel cannot enable live work at a new location merely because
/// this profile once permitted the same model elsewhere. Moving a project
/// requires explicit re-enrollment. This is a permission reference, NOT an
/// authentication boundary against someone who can alter this profile.
pub fn project_binding(root: &Path) -> Result<String> {
    let canonical = std::fs::canonicalize(root)?;
    let mut bytes = canonical.as_os_str().as_encoded_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(CONSENT_VERSION.as_bytes());
    Ok(super::hex_digest(&bytes))
}
