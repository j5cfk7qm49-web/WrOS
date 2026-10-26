//! Exact-version Python prose services, bounded local subprocess protocol.
//! No persistent model process or source-bearing temp file. The caller runs this
//! off the input thread. A separate, single-use source/config-bound permit is
//! mandatory for Nabokov's trained spaCy pipeline; ordinary spelling/proselint
//! do not depend on it. Child outputs are untrusted until fully checked.
use crate::{
    diagnostics::{self, Category, Context, Diagnostic, Profile, RuleKey, Snapshot, SourceStamp},
    prose::Options,
};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const PYTHON_CODE: &str = include_str!("../assets/writingdeck/prose_tools.py");
pub const MAX_STDOUT: usize = 2_097_152;
pub const MAX_WALL_MS: u64 = 60_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Proselint,
    Nabokov,
}
impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Proselint => "proselint",
            Self::Nabokov => "nabokov",
        }
    }
    pub fn version(self) -> &'static str {
        match self {
            Self::Proselint => "0.16.0-dbed789caae6/adapter1",
            Self::Nabokov => "26.7.10-e6f98ce64b52/spacy3.8.16/en3.8.0/adapter1",
        }
    }
    pub fn uses_model(self) -> bool {
        self == Self::Nabokov
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Complete,
    NoProse,
    TooLarge,
    Unavailable,
    Failed,
    TimedOut,
    Cancelled,
    PermissionRequired,
}
#[derive(Debug, Clone)]
pub struct Report {
    pub engine: Engine,
    pub snapshot: Snapshot,
    pub configuration: String,
    pub status: Status,
    pub findings: Vec<Diagnostic>,
    pub omitted: usize,
    pub elapsed_ms: u64,
    pub peak_rss_kib: u64,
    pub notice: String,
}
impl Report {
    pub fn matches(&self, snapshot: &Snapshot, options: &Options) -> bool {
        self.snapshot.stamp == snapshot.stamp && self.configuration == options.digest()
    }
}
#[derive(Debug, Clone)]
pub struct Runtime {
    pub python: PathBuf,
    pub wall_ms: u64,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            python: std::env::var_os("WRITINGDECK_TOOLS_PYTHON")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/opt/writingdeck/python/bin/python3")),
            wall_ms: MAX_WALL_MS,
        }
    }
}
/// A UI/CLI action may create one permit for exactly one existing snapshot.
/// It cannot be serialized, reused, transferred to another document or silently
/// promoted to live analysis. A failed launch also consumes it: retry is explicit.
pub struct ModelPermit {
    source: SourceStamp,
    configuration: String,
    used: AtomicBool,
}
impl ModelPermit {
    pub fn allow_once(snapshot: &Snapshot, options: &Options) -> Self {
        Self {
            source: snapshot.stamp.clone(),
            configuration: options.digest(),
            used: AtomicBool::new(false),
        }
    }
    fn consume(&self, snapshot: &Snapshot, options: &Options) -> bool {
        self.source == snapshot.stamp
            && self.configuration == options.digest()
            && !self.used.swap(true, Ordering::AcqRel)
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema: u32,
    engine: Engine,
    version: String,
    request_id: u64,
    sha256: String,
    status: String,
    findings: Vec<WireFinding>,
    omitted: usize,
    elapsed_ms: u64,
    peak_rss_kib: u64,
    #[serde(default)]
    reason: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFinding {
    rule: String,
    category: Category,
    start: usize,
    end: usize,
    expected: String,
    message: String,
    alternatives: Vec<String>,
}
fn empty(
    engine: Engine,
    snapshot: Snapshot,
    options: &Options,
    status: Status,
    notice: &str,
) -> Report {
    Report {
        engine,
        snapshot,
        configuration: options.digest(),
        status,
        findings: vec![],
        omitted: 0,
        elapsed_ms: 0,
        peak_rss_kib: 0,
        notice: notice.into(),
    }
}
pub fn analyze(
    snapshot: Snapshot,
    options: &Options,
    engine: Engine,
    runtime: &Runtime,
    permit: Option<&ModelPermit>,
    cancel: Arc<AtomicBool>,
) -> Result<Report> {
    options.analysis.validate()?;
    ensure!(
        (1..=MAX_WALL_MS).contains(&runtime.wall_ms),
        "invalid prose child wall budget"
    );
    if cancel.load(Ordering::Acquire) {
        return Ok(empty(
            engine,
            snapshot,
            options,
            Status::Cancelled,
            "cancelled before dispatch",
        ));
    }
    if snapshot.text().len() > crate::prose::MAX_ANALYSIS_BYTES {
        return Ok(empty(
            engine,
            snapshot,
            options,
            Status::TooLarge,
            "source exceeds 65,536-byte service budget; nothing analyzed",
        ));
    }
    let projection = snapshot.project(&options.analysis);
    if !projection.has_prose() {
        return Ok(empty(
            engine,
            snapshot,
            options,
            Status::NoProse,
            "no opted-in prose; model not initialized",
        ));
    }
    if engine.uses_model() && !permit.is_some_and(|permit| permit.consume(&snapshot, options)) {
        return Ok(empty(
            engine,
            snapshot,
            options,
            Status::PermissionRequired,
            "Nabokov uses trained spaCy inference. Allow once for this exact source/configuration before dispatch.",
        ));
    }
    let digest = diagnostics::hex_digest(projection.prose.as_bytes());
    let payload = serde_json::to_vec(
        &serde_json::json!({"schema":1,"engine":engine,"request_id":snapshot.stamp.generation,"sha256":digest,"text":projection.prose.as_ref()}),
    )?;
    let started = Instant::now();
    let mut child = match Command::new(&runtime.python)
        .args(["-I", "-B", "-c", PYTHON_CODE])
        .current_dir("/")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("WRITINGDECK_PARENT_PID", std::process::id().to_string())
        .env("LANG", "C.UTF-8")
        .env("OPENBLAS_NUM_THREADS", "1")
        .env("OMP_NUM_THREADS", "1")
        .env("MKL_NUM_THREADS", "1")
        .env("NUMEXPR_NUM_THREADS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return Ok(empty(
                engine,
                snapshot,
                options,
                Status::Unavailable,
                "pinned local Python runtime unavailable; no dependency/model download attempted",
            ));
        }
    };
    let mut input = child.stdin.take().context("missing child stdin")?;
    let output = child.stdout.take().context("missing child stdout")?;
    // Drain and feed independently so a full pipe/slow startup cannot defeat
    // timeout/cancellation. Both bounded I/O threads end when the child exits.
    let writer = thread::spawn(move || input.write_all(&payload));
    let reader = thread::spawn(move || {
        let mut bytes = vec![];
        output
            .take((MAX_STDOUT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let terminal = loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            break Status::Cancelled;
        }
        if started.elapsed().as_millis() >= runtime.wall_ms as u128 {
            let _ = child.kill();
            let _ = child.wait();
            break Status::TimedOut;
        }
        match child.try_wait() {
            Ok(Some(exit)) => {
                break if exit.success() {
                    Status::Complete
                } else {
                    Status::Failed
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Status::Failed;
            }
        }
    };
    let wrote = writer.join().ok().is_some_and(|r| r.is_ok());
    let bytes = reader.join().ok().and_then(Result::ok);
    if terminal != Status::Complete || !wrote {
        return Ok(empty(
            engine,
            snapshot,
            options,
            if terminal == Status::Complete {
                Status::Failed
            } else {
                terminal
            },
            "offline child did not complete; no clean report or automatic retry",
        ));
    }
    let Some(bytes) = bytes.filter(|b| b.len() <= MAX_STDOUT) else {
        return Ok(empty(
            engine,
            snapshot,
            options,
            Status::Failed,
            "oversized/invalid child output rejected",
        ));
    };
    match decode(&bytes, &snapshot, options, engine, &digest) {
        Ok(mut report) => {
            report.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            Ok(report)
        }
        Err(_) => Ok(empty(
            engine,
            snapshot,
            options,
            Status::Failed,
            "invalid/version-mismatched child output rejected; no findings applied",
        )),
    }
}
fn decode(
    bytes: &[u8],
    snapshot: &Snapshot,
    options: &Options,
    engine: Engine,
    digest: &str,
) -> Result<Report> {
    let wire: Wire = serde_json::from_slice(bytes)?;
    ensure!(
        wire.schema == 1 && wire.engine == engine && wire.version == engine.version(),
        "unsupported prose protocol/engine version"
    );
    ensure!(
        wire.request_id == snapshot.stamp.generation && wire.sha256 == digest,
        "stale or foreign prose response"
    );
    ensure!(
        wire.findings.len() <= diagnostics::MAX_FINDINGS
            && wire.omitted <= 1_000_000
            && wire.elapsed_ms <= 300_000
            && wire.peak_rss_kib <= 4_194_304,
        "invalid prose service bounds"
    );
    ensure!(
        wire.reason
            .as_ref()
            .is_none_or(|r| r.len() <= 256 && !r.chars().any(char::is_control)),
        "invalid engine status"
    );
    let status = match wire.status.as_str() {
        "complete" => Status::Complete,
        "unavailable" => Status::Unavailable,
        "failed" => Status::Failed,
        _ => anyhow::bail!("unknown engine status"),
    };
    ensure!(
        status == Status::Complete || wire.findings.is_empty(),
        "failed engine must not publish findings"
    );
    let mut report = empty(
        engine,
        snapshot.clone(),
        options,
        status,
        wire.reason
            .as_deref()
            .unwrap_or("complete local advisory report"),
    );
    report.omitted = wire.omitted;
    report.peak_rss_kib = wire.peak_rss_kib;
    report.elapsed_ms = wire.elapsed_ms;
    let projection = snapshot.project(&options.analysis);
    for finding in wire.findings {
        ensure!(
            finding.start < finding.end
                && finding.expected.len() <= crate::prose::MAX_ANALYSIS_BYTES
                && finding.alternatives.len() <= 8,
            "invalid finding bounds"
        );
        ensure!(
            matches!(
                finding.category,
                Category::Style | Category::Readability | Category::Usage
            ),
            "unapproved prose category"
        );
        if engine == Engine::Nabokov {
            ensure!(
                matches!(
                    finding.rule.as_str(),
                    "NB201"
                        | "NB202"
                        | "NB203"
                        | "NB301"
                        | "NB302"
                        | "NB303"
                        | "NB304"
                        | "NB305"
                        | "NB401"
                ),
                "detector/authorship or unknown Nabokov rule rejected"
            );
        }
        let key = RuleKey {
            engine: engine.name().into(),
            version: engine.version().into(),
            rule: finding.rule,
        };
        key.validate()?;
        let span = projection
            .scalar_span(finding.start..finding.end)
            .context("unmapped Python character range")?;
        // The masked surface has the same scalar positions, not the same UTF-8
        // byte lengths. Check expected text against *that* surface before the
        // original-source diagnostic decides literal replacement safety.
        let expected: String = projection
            .prose
            .chars()
            .skip(finding.start)
            .take(finding.end - finding.start)
            .collect();
        ensure!(
            expected == finding.expected,
            "upstream span/expected-text disagreement"
        );
        if options.analysis.disabled_rules.contains(&key) {
            continue;
        }
        if options.analysis.profile == Profile::Drafting && finding.category != Category::Usage {
            continue;
        }
        if options.analysis.relaxed_dialogue
            && projection.context(span) == Context::Dialogue
            && matches!(finding.category, Category::Style | Category::Readability)
        {
            continue;
        }
        let diagnostic = Diagnostic::new(
            &projection,
            &options.analysis,
            key,
            finding.category,
            span,
            finding.message,
            finding.alternatives,
        )?;
        report.findings.push(diagnostic);
    }
    report
        .findings
        .sort_by_key(|d| (d.span.start, d.span.end, d.rule.rule.clone()));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{SourceRole, SourceUnit};
    use std::path::Path;
    fn source(text: &str) -> Snapshot {
        Snapshot::new(
            Path::new("scene.md"),
            SourceUnit::EditorBody,
            SourceRole::Manuscript,
            7,
            text,
        )
        .unwrap()
    }
    fn revision() -> Options {
        Options {
            analysis: diagnostics::AnalysisOptions {
                profile: Profile::Revision,
                relaxed_dialogue: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn fixture(snapshot: &Snapshot, options: &Options) -> (serde_json::Value, String) {
        let projection = snapshot.project(&options.analysis);
        let digest = diagnostics::hex_digest(projection.prose.as_bytes());
        let start = projection.prose.chars().position(|c| c == 'i').unwrap();
        let expected = "in order to";
        (
            serde_json::json!({"schema":1,"engine":"nabokov","version":Engine::Nabokov.version(),"request_id":7,"sha256":digest,"status":"complete","findings":[{"rule":"NB401","category":"style","start":start,"end":start+expected.len(),"expected":expected,"message":"Injected wire contract, not a real model result","alternatives":["to"]}],"omitted":0,"elapsed_ms":1,"peak_rss_kib":1}),
            digest,
        )
    }
    #[test]
    fn injected_character_wire_contract_maps_unicode_to_exact_source_bytes_and_rejects_stale_versions()
     {
        let snapshot = source("Café 🦊 e\u{301}. in order to see.");
        let options = revision();
        let (mut wire, digest) = fixture(&snapshot, &options);
        let report = decode(
            &serde_json::to_vec(&wire).unwrap(),
            &snapshot,
            &options,
            Engine::Nabokov,
            &digest,
        )
        .unwrap();
        assert_eq!(snapshot.slice(report.findings[0].span), Some("in order to"));
        assert_eq!(
            report.findings[0]
                .checked_replacement(&snapshot, &options.analysis, 0)
                .unwrap()
                .1,
            "to"
        );
        wire["request_id"] = 8.into();
        assert!(
            decode(
                &serde_json::to_vec(&wire).unwrap(),
                &snapshot,
                &options,
                Engine::Nabokov,
                &digest
            )
            .is_err()
        );
        wire["request_id"] = 7.into();
        wire["version"] = "26.7.10-with-unknown-semantics".into();
        assert!(
            decode(
                &serde_json::to_vec(&wire).unwrap(),
                &snapshot,
                &options,
                Engine::Nabokov,
                &digest
            )
            .is_err()
        );
    }
    #[test]
    fn detector_rules_unknown_fields_and_false_clean_statuses_are_rejected() {
        let snapshot = source("in order to see.");
        let options = revision();
        let (mut wire, digest) = fixture(&snapshot, &options);
        wire["findings"][0]["rule"] = "NB513".into();
        assert!(
            decode(
                &serde_json::to_vec(&wire).unwrap(),
                &snapshot,
                &options,
                Engine::Nabokov,
                &digest
            )
            .is_err()
        );
        wire["findings"][0]["rule"] = "NB401".into();
        wire["score"] = 99.into();
        assert!(
            decode(
                &serde_json::to_vec(&wire).unwrap(),
                &snapshot,
                &options,
                Engine::Nabokov,
                &digest
            )
            .is_err()
        );
        wire.as_object_mut().unwrap().remove("score");
        wire["status"] = "failed".into();
        assert!(
            decode(
                &serde_json::to_vec(&wire).unwrap(),
                &snapshot,
                &options,
                Engine::Nabokov,
                &digest
            )
            .is_err()
        );
    }
    #[test]
    fn single_use_model_permission_cannot_follow_changed_config_or_source() {
        let snapshot = source("in order to see.");
        let options = revision();
        let permit = ModelPermit::allow_once(&snapshot, &options);
        let mut changed = options.clone();
        changed.preset = crate::prose::FictionPreset::Experimental;
        assert!(!permit.consume(&snapshot, &changed));
        assert!(permit.consume(&snapshot, &options));
        assert!(!permit.consume(&snapshot, &options));
        let other = source("different in order to see.");
        let permit = ModelPermit::allow_once(&snapshot, &options);
        assert!(!permit.consume(&other, &options));
        assert!(permit.consume(&snapshot, &options));
    }
    #[test]
    fn no_permission_or_excluded_prose_never_launches_even_an_invalid_runtime() {
        let runtime = Runtime {
            python: PathBuf::from("/this-runtime-must-not-be-run"),
            wall_ms: 100,
        };
        let options = revision();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = analyze(
            source("in order to see."),
            &options,
            Engine::Nabokov,
            &runtime,
            None,
            cancel.clone(),
        )
        .unwrap();
        assert_eq!(result.status, Status::PermissionRequired);
        let note = Snapshot::new(
            Path::new("notes/n.md"),
            SourceUnit::EditorBody,
            SourceRole::Note,
            7,
            "private reference",
        )
        .unwrap();
        let result = analyze(note, &options, Engine::Nabokov, &runtime, None, cancel).unwrap();
        assert_eq!(result.status, Status::NoProse);
    }
    #[test]
    fn unavailable_local_runtime_never_becomes_empty_clean_success() {
        let result = analyze(
            source("in order to see."),
            &revision(),
            Engine::Proselint,
            &Runtime {
                python: PathBuf::from("/not-installed-python"),
                wall_ms: 100,
            },
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(result.status, Status::Unavailable);
        assert!(result.findings.is_empty());
    }
    #[test]
    fn cancellation_before_dispatch_is_not_a_clean_report() {
        let result = analyze(
            source("in order to see."),
            &revision(),
            Engine::Proselint,
            &Runtime::default(),
            None,
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap();
        assert_eq!(result.status, Status::Cancelled);
    }
}
