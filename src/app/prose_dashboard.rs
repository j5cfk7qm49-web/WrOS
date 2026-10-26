//! Explicit, cache-only revision reader over the existing exclusive editors.
//! One active job and one newest queued request; no editor or manuscript save
//! owner here. Model permission is separate from Harper and never persisted.
use super::*;
use anyhow::{bail, ensure};
use grimoire_core::{
    diagnostics::{self as diag, Diagnostic, Profile, RuleKey, Snapshot, SourceRole, SourceUnit},
    prose, prose_tools as tools, review_queue as review,
    statefile::Store,
    voice_profile as voice,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use std::{
    cell::Cell,
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Summary,
    Queue,
    Detail,
    ModelConsent,
    Voice,
    Sources,
    VoiceName,
    DeleteVoice,
    GlobalMute,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Open,
    All,
    Grammar,
    Spelling,
    Style,
    Readability,
    Reviewed,
}
impl Filter {
    fn next(self) -> Self {
        match self {
            Self::Open => Self::All,
            Self::All => Self::Grammar,
            Self::Grammar => Self::Spelling,
            Self::Spelling => Self::Style,
            Self::Style => Self::Readability,
            Self::Readability => Self::Reviewed,
            Self::Reviewed => Self::Open,
        }
    }
    fn accepts(self, item: &review::Item) -> bool {
        match self {
            Self::Open => item.state.is_none(),
            Self::All => true,
            Self::Grammar => matches!(
                item.priority,
                review::Priority::Grammar | review::Priority::Usage
            ),
            Self::Spelling => item.priority == review::Priority::Spelling,
            Self::Style => item.priority == review::Priority::Style,
            Self::Readability => item.priority == review::Priority::Readability,
            Self::Reviewed => item.state.is_some(),
        }
    }
}
#[derive(Clone)]
pub struct Binding {
    pub pane: Pane,
    pub document: String,
    pub epoch: (u64, u64),
    pub configuration: String,
    pub spelling_language: spell::Language,
}
pub struct Analysis {
    pub binding: Binding,
    pub native: prose::Report,
    pub services: Vec<tools::Report>,
    pub queue: review::Queue,
    pub harper_cached: bool,
    pub spelling_count: usize,
}
struct ReferenceSource {
    snapshot: Snapshot,
    path: PathBuf,
    seen: grimoire_core::sync::Fingerprint,
}
enum Work {
    Analyze {
        snapshot: Snapshot,
        binding: Binding,
        options: prose::Options,
        words: Vec<String>,
        language: spell::Language,
        runtime: tools::Runtime,
        permit: Option<Box<tools::ModelPermit>>,
        harper: Vec<Diagnostic>,
        harper_cached: bool,
    },
    Voice {
        sources: Vec<ReferenceSource>,
        options: prose::Options,
        label: String,
        stamp: String,
    },
}
enum Outcome {
    Analyze(Box<Analysis>),
    Voice(Box<voice::Prepared>),
    Failed(String),
}
struct Active {
    id: u64,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Outcome>,
}
pub struct Dashboard {
    pub page: Page,
    pub filter: Filter,
    pub selected: usize,
    pub alternative: usize,
    pub scroll: Option<usize>,
    pub fit: Cell<(usize, usize)>,
    pub result: Option<Analysis>,
    pub settings: Option<Store<prose::Settings>>,
    pub reviews: Option<Store<review::Review>>,
    pub voices: Option<Store<voice::Library>>,
    pub warning: String,
    pub notice: String,
    pub runtime: tools::Runtime,
    pub launched: u64,
    pub discarded: u64,
    pub sources: Vec<usize>,
    pub chosen: BTreeSet<usize>,
    pub name: String,
    pub prepared: Option<voice::Prepared>,
    active: Option<Active>,
    queued: Option<(u64, Work)>,
    generation: u64,
    once: BTreeSet<String>,
    pending_rule: Option<RuleKey>,
    pending_profile: Option<u64>,
}
impl Dashboard {
    pub fn load(root: &Path) -> Self {
        let mut warnings = vec![];
        let settings = match prose::Settings::load(root) {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!("Prose settings: {e:#}; original preserved."));
                None
            }
        };
        let reviews = match review::load(root) {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!("Review metadata: {e:#}; original preserved."));
                None
            }
        };
        let voices = match voice::load(root) {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!("Voice metadata: {e:#}; original preserved."));
                None
            }
        };
        Self{page:Page::Summary,filter:Filter::Open,selected:0,alternative:0,scroll:None,fit:Cell::new((0,0)),result:None,settings,reviews,voices,warning:warnings.join(" "),notice:"No analysis started. r explicitly runs native patterns, independent spelling and pinned proselint. d offers separate one-run model permission.".into(),runtime:tools::Runtime::default(),launched:0,discarded:0,sources:vec![],chosen:BTreeSet::new(),name:String::new(),prepared:None,active:None,queued:None,generation:0,once:BTreeSet::new(),pending_rule:None,pending_profile:None}
    }
    pub fn preset(&self) -> prose::FictionPreset {
        self.settings
            .as_ref()
            .map_or(prose::FictionPreset::General, |s| s.value.preset)
    }
    pub fn pending(&self) -> bool {
        self.active.is_some() || self.queued.is_some()
    }
    fn cancel_pending(&mut self) {
        self.generation = self.generation.saturating_add(1);
        if let Some(active) = &self.active {
            active.cancel.store(true, Ordering::Release);
        }
        self.queued = None;
    }
    pub fn invalidate(&mut self) {
        self.cancel_pending();
        self.result = None;
        self.once.clear();
        self.selected = 0;
        self.alternative = 0;
        self.scroll = None;
    }
    fn request(&mut self, work: Work) {
        self.invalidate();
        let id = self.generation;
        if self.active.is_some() {
            self.queued = Some((id, work));
            self.notice = "Newest explicit request queued; cancelling the older worker.".into();
        } else {
            self.launch(id, work);
        }
    }
    fn launch(&mut self, id: u64, work: Work) {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.launched += 1;
        std::thread::spawn(move || {
            let result =
                catch_unwind(AssertUnwindSafe(|| run_work(work, signal))).unwrap_or_else(|_| {
                    Err(anyhow::anyhow!("prose worker failed; originals unchanged"))
                });
            let _ = tx.send(result.unwrap_or_else(|e| Outcome::Failed(format!("{e:#}"))));
        });
        self.active = Some(Active {
            id,
            cancel,
            receiver: rx,
        });
        self.notice="Explicit local analysis pending; input remains responsive. No typing/active-time credit is inferred.".into();
    }
    pub fn visible(&self) -> Vec<&review::Item> {
        self.result
            .iter()
            .flat_map(|r| &r.queue.items)
            .filter(|item| self.filter.accepts(item) && !self.once.contains(&item.id))
            .collect()
    }
    fn selected_item(&self) -> Option<&review::Item> {
        let rows = self.visible();
        rows.get(self.selected.min(rows.len().saturating_sub(1)))
            .copied()
    }
}
impl Drop for Dashboard {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.cancel.store(true, Ordering::Release);
        }
    }
}
fn run_work(work: Work, cancel: Arc<AtomicBool>) -> Result<Outcome> {
    match work {
        Work::Analyze {
            snapshot,
            binding,
            options,
            words,
            language,
            runtime,
            permit,
            harper,
            harper_cached,
        } => {
            let native = prose::analyze(snapshot.clone(), options.clone())?;
            let projection = snapshot.project(&options.analysis);
            let mut findings = native.findings.clone();
            let mut spelling_count = 0;
            if native.status == prose::Status::Complete && !cancel.load(Ordering::Acquire) {
                let mut dictionary = spell::Speller::for_language(language);
                let vocab = diag::hex_digest(words.join("\n").as_bytes());
                dictionary.add_words(words);
                let mut origin = 0;
                let version = format!("0.4.2/{}/{:.16}", language.key(), vocab);
                for line in projection.prose.split_inclusive('\n') {
                    for (from, to) in dictionary.misspellings(line) {
                        spelling_count += 1;
                        if spelling_count <= diag::MAX_FINDINGS
                            && let Some(span) = projection.scalar_span(origin + from..origin + to)
                            && projection.replaceable_span(span)
                        {
                            let key = RuleKey {
                                engine: "spellbook".into(),
                                version: version.clone(),
                                rule: "dictionary".into(),
                            };
                            if !options.analysis.disabled_rules.contains(&key) {
                                findings.push(Diagnostic::new(&projection,&options.analysis,key,diag::Category::Usage,span,"Possible dictionary misspelling. s opens the existing spelling suggestions at the original word; names/deliberate forms may be valid.".into(),vec![])?);
                            }
                        }
                    }
                    origin += line.chars().count();
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                }
            }
            // Only immediately validated, same-owner, same-config Harper cache
            // is admitted by the controller. Recreate against this exact current
            // snapshot; never transfer an in-flight/older edited-buffer result.
            for cached in harper {
                ensure!(
                    cached.source.document == snapshot.stamp.document
                        && cached.source.sha256 == snapshot.stamp.sha256
                        && cached.source.unit == snapshot.stamp.unit
                        && cached.source.role == snapshot.stamp.role
                        && cached.configuration == options.analysis.digest()
                        && snapshot.slice(cached.span) == Some(cached.expected.as_str()),
                    "foreign/stale Harper cache refused"
                );
                findings.push(Diagnostic::new(
                    &projection,
                    &options.analysis,
                    cached.rule,
                    cached.category,
                    cached.span,
                    cached.message,
                    cached.replacements.into_iter().map(|r| r.text).collect(),
                )?);
            }
            let mut services = vec![];
            if !cancel.load(Ordering::Acquire) {
                let report = tools::analyze(
                    snapshot.clone(),
                    &options,
                    tools::Engine::Proselint,
                    &runtime,
                    None,
                    cancel.clone(),
                )?;
                findings.extend(report.findings.iter().cloned());
                services.push(report);
            }
            if let Some(permit) = permit
                && !cancel.load(Ordering::Acquire)
            {
                let report = tools::analyze(
                    snapshot,
                    &options,
                    tools::Engine::Nabokov,
                    &runtime,
                    Some(&permit),
                    cancel,
                )?;
                findings.extend(report.findings.iter().cloned());
                services.push(report);
            }
            let queue = review::Queue::build(findings, &review::Review::default())?;
            Ok(Outcome::Analyze(Box::new(Analysis {
                binding,
                native,
                services,
                queue,
                harper_cached,
                spelling_count,
            })))
        }
        Work::Voice {
            sources,
            options,
            label,
            stamp,
        } => {
            let mut snapshots = vec![];
            for source in &sources {
                ensure!(
                    !cancel.load(Ordering::Acquire),
                    "reference preparation cancelled"
                );
                ensure!(
                    grimoire_core::sync::Fingerprint::read(&source.path) == Some(source.seen),
                    "reference backing source changed/unavailable; original retained"
                );
                snapshots.push(source.snapshot.clone());
            }
            let prepared = voice::prepare(&label, &snapshots, options, &stamp)?;
            for source in &sources {
                ensure!(
                    grimoire_core::sync::Fingerprint::read(&source.path) == Some(source.seen),
                    "reference changed during preparation; snapshot refused"
                );
            }
            Ok(Outcome::Voice(Box::new(prepared)))
        }
    }
}
impl App {
    pub fn open_prose_dashboard(&mut self) {
        self.overlay = Overlay::ProseDashboard;
        self.prose_dashboard.page = Page::Summary;
        self.prose_dashboard.scroll = None;
    }
    fn prose_options(&self) -> prose::Options {
        prose::Options {
            analysis: self.checks.options(),
            preset: self.prose_dashboard.preset(),
        }
    }
    fn prose_binding_matches(&self, binding: &Binding) -> bool {
        self.views.active == binding.pane
            && self.editor.revision_stamp() == binding.epoch
            && self.spell_lang == binding.spelling_language
            && self.prose_options().digest() == binding.configuration
            && self
                .check_document(binding.pane)
                .is_ok_and(|(_, d)| d == binding.document)
    }
    pub fn current_prose_analysis(&self) -> Option<&Analysis> {
        self.prose_dashboard
            .result
            .as_ref()
            .filter(|r| self.prose_binding_matches(&r.binding))
    }
    fn request_prose_analysis(&mut self, model: bool) -> Result<()> {
        self.ensure_document_claims()?;
        let id = self.prose_dashboard.generation.saturating_add(1);
        let snapshot = self.check_snapshot(self.views.active, id)?;
        let options = self.prose_options();
        let cached = self.current_check_report().filter(|r| {
            r.options == options.analysis && r.status == diag::harper::ReportStatus::Complete
        });
        let harper_cached = cached.is_some();
        let harper = cached.map_or_else(Vec::new, |r| r.findings.clone());
        let binding = Binding {
            pane: self.views.active,
            document: snapshot.stamp.document.clone(),
            epoch: self.editor.revision_stamp(),
            configuration: options.digest(),
            spelling_language: self.spell_lang,
        };
        let permit = model.then(|| Box::new(tools::ModelPermit::allow_once(&snapshot, &options)));
        let words = self.speller_words();
        let language = self.spell_lang;
        let runtime = self.prose_dashboard.runtime.clone();
        self.prose_dashboard.request(Work::Analyze {
            snapshot,
            binding,
            options,
            words,
            language,
            runtime,
            permit,
            harper,
            harper_cached,
        });
        self.prose_dashboard.page = Page::Summary;
        Ok(())
    }
    pub fn tick_prose_dashboard(&mut self) {
        if self
            .prose_dashboard
            .result
            .as_ref()
            .is_some_and(|r| !self.prose_binding_matches(&r.binding))
        {
            self.prose_dashboard.invalidate();
            self.prose_dashboard.notice="Source owner/revision/settings changed. Even Undo restoring identical bytes requires r again; stale findings are hidden.".into();
        }
        let Some(active) = &self.prose_dashboard.active else {
            return;
        };
        let id = active.id;
        let result = match active.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some(Outcome::Failed(
                "prose worker disconnected; no clean report".into(),
            )),
            Err(TryRecvError::Empty) => None,
        };
        let Some(result) = result else { return };
        self.prose_dashboard.active = None;
        if id != self.prose_dashboard.generation {
            self.prose_dashboard.discarded += 1;
        } else {
            match result {
                Outcome::Analyze(result) if self.prose_binding_matches(&result.binding) => {
                    let mut result = *result;
                    let reviews = self
                        .prose_dashboard
                        .reviews
                        .as_ref()
                        .map_or_else(review::Review::default, |s| s.value.clone());
                    for item in &mut result.queue.items {
                        item.state = reviews
                            .receipts
                            .iter()
                            .find(|r| r.id == item.id)
                            .map(|r| r.state);
                    }
                    self.prose_dashboard.result = Some(result);
                    self.prose_dashboard.notice="Current source results ready. Coverage/status is explicit; missing engines are not a clean check. Enter reviews statistics and opens the queue.".into();
                }
                Outcome::Analyze(_) => {
                    self.prose_dashboard.discarded += 1;
                    self.prose_dashboard.notice =
                        "Older source result discarded; r checks the current owner/revision."
                            .into();
                }
                Outcome::Voice(prepared) => {
                    self.prose_dashboard.prepared = Some(*prepared);
                    self.prose_dashboard.page = Page::Voice;
                    self.prose_dashboard.notice="Frozen selected-reference statistics ready; s explicitly saves metadata. No source was rewritten.".into();
                }
                Outcome::Failed(error) => self.prose_dashboard.notice = error,
            }
        }
        if let Some((id, work)) = self.prose_dashboard.queued.take() {
            self.prose_dashboard.launch(id, work);
        }
    }
    /// Reuse the native editor's source-bound underline layer; no rendered
    /// text offsets and no independent edit buffer are introduced.
    pub fn prose_line_spans(&self) -> std::collections::BTreeMap<usize, Vec<(usize, usize)>> {
        let mut out = std::collections::BTreeMap::<usize, Vec<(usize, usize)>>::new();
        let Some(report) = self.current_prose_analysis() else {
            return out;
        };
        for item in &report.queue.items {
            if item.state.is_some() || self.prose_dashboard.once.contains(&item.id) {
                continue;
            }
            let (Some((line, col)), Some((last, end))) = (
                report.native.snapshot.point(item.finding.span.start),
                report.native.snapshot.point(item.finding.span.end),
            ) else {
                continue;
            };
            for row in line..=last {
                let from = if row == line { col } else { 0 };
                let to = if row == last {
                    end
                } else {
                    self.editor.lines.get(row).map_or(0, |s| s.chars().count())
                };
                if from < to {
                    out.entry(row).or_default().push((from, to));
                }
            }
        }
        for spans in out.values_mut() {
            spans.sort();
            spans.dedup();
        }
        out
    }
    fn selected_prose_item(&self) -> Result<review::Item> {
        ensure!(
            self.current_prose_analysis().is_some(),
            "no current source result; r checks this revision"
        );
        self.prose_dashboard
            .selected_item()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no selected current finding"))
    }
    fn goto_prose_item(&mut self) -> Result<()> {
        let item = self.selected_prose_item()?;
        let source = &self.current_prose_analysis().unwrap().native.snapshot;
        let from = source
            .point(item.finding.span.start)
            .ok_or_else(|| anyhow::anyhow!("invalid source start"))?;
        let to = source
            .point(item.finding.span.end)
            .ok_or_else(|| anyhow::anyhow!("invalid source end"))?;
        self.editor.select(from, to);
        self.focus = self.editor_focus();
        self.overlay = Overlay::None;
        Ok(())
    }
    fn apply_prose_item(&mut self) -> Result<()> {
        self.ensure_document_claims()?;
        let item = self.selected_prose_item()?;
        let analysis = self.current_prose_analysis().unwrap();
        let current =
            self.check_snapshot(self.views.active, analysis.native.snapshot.stamp.generation)?;
        let options = self.prose_options();
        ensure!(
            analysis.native.matches(&current, &options),
            "stale native configuration"
        );
        ensure!(
            item.finding.expected.chars().count() <= 512,
            "broad source span: inspect/edit the original instead of one-click replacement"
        );
        let (span, replacement) = item.finding.checked_replacement(
            &current,
            &options.analysis,
            self.prose_dashboard.alternative,
        )?;
        let replacement = replacement.to_owned();
        let (i, _) = self.check_document(self.views.active)?;
        let node = &self.project.nodes[i];
        ensure!(
            node.seen.is_some() && grimoire_core::sync::Fingerprint::read(&node.path) == node.seen,
            "backing source changed/unreadable; reconcile first, no replacement applied"
        );
        let from = current
            .point(span.start)
            .ok_or_else(|| anyhow::anyhow!("unmapped source start"))?;
        let to = current
            .point(span.end)
            .ok_or_else(|| anyhow::anyhow!("unmapped source end"))?;
        self.editor.select(from, to);
        self.editor
            .replace_selection_with_as(&replacement, grimoire_core::measure::Origin::Replace);
        self.flush();
        self.focus = self.editor_focus();
        self.prose_dashboard.invalidate();
        self.prose_dashboard.page = Page::Summary;
        self.prose_dashboard.notice="One explicit original-source Replace. Native Undo remains available after Esc; no typing credit and no automatic next fix. r checks the new revision.".into();
        Ok(())
    }
    fn record_prose_item(&mut self, state: review::State) -> Result<()> {
        let item = self.selected_prose_item()?;
        let stamp = self.productivity.clock.now().stamp();
        let store =
            self.prose_dashboard.reviews.as_mut().ok_or_else(|| {
                anyhow::anyhow!("review metadata unavailable; original preserved")
            })?;
        review::record(store, &item, state, &stamp)?;
        if let Some(result) = &mut self.prose_dashboard.result
            && let Some(row) = result.queue.items.iter_mut().find(|r| r.id == item.id)
        {
            row.state = Some(state);
        }
        self.prose_dashboard.page = Page::Queue;
        self.prose_dashboard.selected = 0;
        self.prose_dashboard.notice = format!(
            "{state:?} receipt saved as source/rule/hash references only; manuscript untouched."
        );
        Ok(())
    }
    fn start_voice_sources(&mut self) {
        self.prose_dashboard.sources = self
            .project
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                n.kind == Kind::Scene
                    && n.in_manuscript
                    && !n.parked
                    && !n.read_only
                    && !self.project.in_trash(*i)
                    && !n.never_read()
            })
            .map(|(i, _)| i)
            .collect();
        self.prose_dashboard.chosen.clear();
        self.prose_dashboard.selected = 0;
        self.prose_dashboard.page = Page::Sources;
        self.prose_dashboard.notice="Explicitly choose your own saved manuscript material. No automatic whole-book sampling or inference of authorship. Space toggles; Enter names the frozen baseline.".into();
    }
    fn prepare_voice_sources(&mut self) -> Result<()> {
        ensure!(
            self.prose_dashboard.voices.is_some(),
            "voice metadata unavailable; original preserved"
        );
        ensure!(
            !self.prose_dashboard.chosen.is_empty()
                && self.prose_dashboard.chosen.len() <= voice::MAX_REFERENCES,
            "choose one to 64 manuscript sources"
        );
        let mut sources = vec![];
        for i in &self.prose_dashboard.chosen {
            let node = self
                .project
                .nodes
                .get(*i)
                .ok_or_else(|| anyhow::anyhow!("source moved; choose again"))?;
            ensure!(
                node.kind == Kind::Scene
                    && node.in_manuscript
                    && !self.project.in_trash(*i)
                    && !node.parked
                    && !node.read_only
                    && !node.dirty
                    && node.disk.unavailable.is_none()
                    && node.disk.pending.is_none(),
                "reference must be an available saved original: save/reconcile first"
            );
            let seen = node
                .seen
                .ok_or_else(|| anyhow::anyhow!("reference backing source unavailable"))?;
            let snapshot = Snapshot::new(
                node.path.strip_prefix(&self.project.root)?,
                SourceUnit::EditorBody,
                SourceRole::Manuscript,
                self.prose_dashboard.generation.saturating_add(1),
                node.body.clone(),
            )?;
            sources.push(ReferenceSource {
                snapshot,
                path: node.path.clone(),
                seen,
            });
        }
        let options = self.prose_options();
        let label = self.prose_dashboard.name.clone();
        let stamp = self.productivity.clock.now().stamp();
        self.prose_dashboard.prepared = None;
        self.prose_dashboard.request(Work::Voice {
            sources,
            options,
            label,
            stamp,
        });
        self.prose_dashboard.page = Page::Voice;
        Ok(())
    }
    fn save_voice_reference(&mut self) -> Result<()> {
        let prepared = self
            .prose_dashboard
            .prepared
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("prepare sufficient selected reference material first"))?
            .clone();
        let store = self
            .prose_dashboard
            .voices
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("voice metadata unavailable; original preserved"))?;
        let id = voice::save(store, prepared)?;
        self.prose_dashboard.prepared = None;
        self.prose_dashboard.notice = format!(
            "Frozen voice reference #{id} saved. Numeric patterns are not literary quality or authorship proof; originals remain authoritative."
        );
        Ok(())
    }
    pub fn paste_prose_dashboard(&mut self, text: &str) {
        if self.prose_dashboard.page != Page::VoiceName {
            self.prose_dashboard.notice =
                "Read-only revision reader: pasted commands ignored.".into();
            return;
        }
        for c in text.chars().filter(|c| !c.is_control()) {
            if self.prose_dashboard.name.chars().count() < 64 {
                self.prose_dashboard.name.push(c);
            }
        }
    }
    pub fn on_prose_dashboard_key(&mut self, key: Key) {
        self.prose_dashboard.scroll = match key {
            Key::PageDown => {
                let (at, max) = self.prose_dashboard.fit.get();
                Some((at + 10).min(max))
            }
            Key::PageUp => Some(self.prose_dashboard.fit.get().0.saturating_sub(10)),
            _ => None,
        };
        if self.prose_dashboard.page == Page::VoiceName {
            let result = match key {
                Key::Esc => {
                    self.prose_dashboard.page = Page::Sources;
                    Ok(())
                }
                Key::Enter => self.prepare_voice_sources(),
                Key::Delete => {
                    self.prose_dashboard.name.clear();
                    Ok(())
                }
                Key::Backspace => {
                    self.prose_dashboard.name.pop();
                    Ok(())
                }
                Key::Char(c) if !c.is_control() => {
                    if self.prose_dashboard.name.chars().count() < 64 {
                        self.prose_dashboard.name.push(c);
                    }
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                self.prose_dashboard.notice = format!("{e:#}");
            }
            return;
        }
        let page = self.prose_dashboard.page;
        let result: Result<()> = match key {
            Key::Esc => {
                match page {
                    Page::Summary => {
                        // Closing stops unpublished work, but a completed,
                        // source-bound cache continues to underline the native
                        // editor. c explicitly clears it; edits mask it at once.
                        self.prose_dashboard.cancel_pending();
                        self.overlay = Overlay::None;
                    }
                    Page::Detail => self.prose_dashboard.page = Page::Queue,
                    Page::Sources | Page::DeleteVoice => self.prose_dashboard.page = Page::Voice,
                    _ => self.prose_dashboard.page = Page::Summary,
                };
                Ok(())
            }
            Key::Char('c') if page == Page::Summary => {
                self.prose_dashboard.invalidate();
                self.prose_dashboard.notice="Cleared cached prose results/underlines and cancelled pending prose work. Independent spelling/Harper controls are unchanged.".into();
                Ok(())
            }
            Key::Char('r') => self.request_prose_analysis(false),
            Key::Char('d') => {
                self.prose_dashboard.page = Page::ModelConsent;
                Ok(())
            }
            Key::Char('y') if page == Page::ModelConsent => self.request_prose_analysis(true),
            Key::Char('g') if page != Page::Detail => {
                self.open_checks();
                Ok(())
            }
            Key::Char('g') if page == Page::Detail => self.goto_prose_item(),
            Key::Char('s') if page == Page::Detail => self
                .goto_prose_item()
                .map(|_| self.run_action(Action::SpellingSuggestions)),
            Key::Char('p') => {
                let result = self.save_check_options(|o| {
                    o.profile = match o.profile {
                        Profile::Drafting => Profile::Revision,
                        Profile::Revision => Profile::DeepReview,
                        Profile::DeepReview => Profile::Drafting,
                    }
                });
                if result.is_ok() {
                    self.prose_dashboard.invalidate();
                }
                result
            }
            Key::Char('l') => {
                let result = self.save_check_options(|o| o.relaxed_dialogue = !o.relaxed_dialogue);
                if result.is_ok() {
                    self.prose_dashboard.invalidate();
                }
                result
            }
            Key::Char('o') => {
                let result =
                    self.save_check_options(|o| o.include_footnotes = !o.include_footnotes);
                if result.is_ok() {
                    self.prose_dashboard.invalidate();
                }
                result
            }
            Key::Char('f') => {
                let result = self
                    .prose_dashboard
                    .settings
                    .as_mut()
                    .ok_or_else(|| {
                        anyhow::anyhow!("prose settings unavailable; original preserved")
                    })
                    .and_then(|s| {
                        s.update(|v| {
                            v.preset = match v.preset {
                                prose::FictionPreset::General => prose::FictionPreset::Accessible,
                                prose::FictionPreset::Accessible => {
                                    prose::FictionPreset::Experimental
                                }
                                prose::FictionPreset::Experimental => prose::FictionPreset::General,
                            };
                            Ok(())
                        })
                    });
                if result.is_ok() {
                    self.prose_dashboard.invalidate();
                }
                result
            }
            Key::Char('v') => {
                self.prose_dashboard.page = Page::Voice;
                self.prose_dashboard.selected = 0;
                Ok(())
            }
            Key::Char('n') if page == Page::Voice => {
                self.start_voice_sources();
                Ok(())
            }
            Key::Char('s') if page == Page::Voice => self.save_voice_reference(),
            Key::Char('c') if page == Page::Voice => {
                let result = (|| -> Result<usize> {
                    let current = self.current_prose_analysis().ok_or_else(|| {
                        anyhow::anyhow!("analyze at least 300 current words/five segments first")
                    })?;
                    let library = &self
                        .prose_dashboard
                        .voices
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("voice metadata unavailable"))?
                        .value;
                    let profile = library
                        .active
                        .and_then(|id| library.profiles.iter().find(|p| p.id == id))
                        .ok_or_else(|| {
                            anyhow::anyhow!("select a saved sufficient-material baseline first")
                        })?;
                    Ok(voice::compare(profile, &current.native)?.len())
                })();
                result.map(|n|{self.prose_dashboard.notice=format!("Reviewed actual reference comparison: {n} informational feature deviations, no total score or authorship proof.");self.observe_deck(grimoire_core::tutorial::Event::VoiceProfileCompared);})
            }

            Key::Char('D') if page == Page::Voice => {
                self.prose_dashboard.pending_profile = self
                    .prose_dashboard
                    .voices
                    .as_ref()
                    .and_then(|s| s.value.profiles.get(self.prose_dashboard.selected))
                    .map(|p| p.id);
                self.prose_dashboard.page = Page::DeleteVoice;
                Ok(())
            }
            Key::Char('y') if page == Page::DeleteVoice => {
                let result = self
                    .prose_dashboard
                    .pending_profile
                    .ok_or_else(|| anyhow::anyhow!("no selected profile"))
                    .and_then(|id| {
                        self.prose_dashboard
                            .voices
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("voice metadata unavailable"))
                            .and_then(|s| voice::remove(s, id))
                    });
                if result.is_ok() {
                    self.prose_dashboard.page = Page::Voice;
                }
                result
            }
            Key::Char(' ') if page == Page::Sources => {
                if let Some(i) = self
                    .prose_dashboard
                    .sources
                    .get(self.prose_dashboard.selected)
                    .copied()
                    && !self.prose_dashboard.chosen.remove(&i)
                {
                    if self.prose_dashboard.chosen.len() < voice::MAX_REFERENCES {
                        self.prose_dashboard.chosen.insert(i);
                    } else {
                        self.prose_dashboard.notice =
                            "64 reference-source cap; choose a bounded representative selection."
                                .into();
                    }
                }
                Ok(())
            }
            Key::Enter if page == Page::Sources => {
                self.prose_dashboard.name = format!(
                    "Reference {}",
                    self.prose_dashboard
                        .voices
                        .as_ref()
                        .map_or(1, |s| s.value.next_id)
                );
                self.prose_dashboard.page = Page::VoiceName;
                Ok(())
            }
            Key::Enter if page == Page::Voice => {
                let id = self
                    .prose_dashboard
                    .voices
                    .as_ref()
                    .and_then(|s| s.value.profiles.get(self.prose_dashboard.selected))
                    .map(|p| p.id);
                id.ok_or_else(|| anyhow::anyhow!("no saved reference; n selects your originals"))
                    .and_then(|id| {
                        self.prose_dashboard
                            .voices
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("voice metadata unavailable"))
                            .and_then(|s| voice::activate(s, id))
                    })
            }
            Key::Enter if page == Page::Summary => {
                if self.current_prose_analysis().is_some_and(|r| {
                    r.native.status == prose::Status::Complete
                        && r.native.metrics.analysis_words > 0
                }) {
                    self.observe_deck(grimoire_core::tutorial::Event::ProseStatsReviewed);
                    self.prose_dashboard.page = Page::Queue;
                    self.prose_dashboard.selected = 0;
                    Ok(())
                } else {
                    bail_result("run and review actual current statistics first")
                }
            }
            Key::Enter if page == Page::Queue => match self.selected_prose_item() {
                Ok(item) => {
                    if item.provenance.iter().any(|r| r.engine == "nabokov") {
                        self.observe_deck(grimoire_core::tutorial::Event::NabokovFindingReviewed);
                    }
                    if item.provenance.iter().any(|r| r.engine == "proselint") {
                        self.observe_deck(grimoire_core::tutorial::Event::ProselintFindingReviewed);
                    }
                    self.prose_dashboard.page = Page::Detail;
                    self.prose_dashboard.alternative = 0;
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Key::Char('a') if page == Page::Detail => self.apply_prose_item(),
            Key::Char('i') if matches!(page, Page::Queue | Page::Detail) => {
                self.selected_prose_item().map(|item| {
                    self.prose_dashboard.once.insert(item.id);
                    self.prose_dashboard.page = Page::Queue;
                    self.prose_dashboard.selected = 0;
                })
            }
            Key::Char('x') if matches!(page, Page::Queue | Page::Detail) => {
                self.record_prose_item(review::State::Dismissed)
            }
            Key::Char('w') if matches!(page, Page::Queue | Page::Detail) => {
                self.record_prose_item(review::State::Reviewed)
            }
            Key::Char('m') if matches!(page, Page::Queue | Page::Detail) => {
                self.selected_prose_item().and_then(|item| {
                    self.save_check_options(|o| {
                        o.disabled_rules.insert(item.finding.rule);
                    })?;
                    self.prose_dashboard.invalidate();
                    Ok(())
                })
            }
            Key::Char('M') if matches!(page, Page::Queue | Page::Detail) => {
                self.selected_prose_item().map(|item| {
                    self.prose_dashboard.pending_rule = Some(item.finding.rule);
                    self.prose_dashboard.page = Page::GlobalMute;
                })
            }
            Key::Char('y') if page == Page::GlobalMute => {
                let result = self
                    .prose_dashboard
                    .pending_rule
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("no selected rule"))
                    .and_then(|rule| {
                        self.checks
                            .personal
                            .as_mut()
                            .ok_or_else(|| {
                                anyhow::anyhow!("personal metadata unavailable; original preserved")
                            })
                            .and_then(|s| {
                                s.update(|p| {
                                    ensure!(p.muted_rules.len() < 256, "personal mute limit");
                                    p.muted_rules.insert(rule);
                                    Ok(())
                                })
                            })
                    });
                if result.is_ok() {
                    self.checks.invalidate();
                    self.prose_dashboard.invalidate();
                    self.prose_dashboard.page = Page::Summary;
                }
                result
            }
            Key::Tab if page == Page::Queue => {
                self.prose_dashboard.filter = self.prose_dashboard.filter.next();
                self.prose_dashboard.selected = 0;
                Ok(())
            }
            Key::Right if page == Page::Detail => {
                let n = self
                    .prose_dashboard
                    .selected_item()
                    .map_or(0, |i| i.finding.replacements.len());
                self.prose_dashboard.alternative =
                    (self.prose_dashboard.alternative + 1).min(n.saturating_sub(1));
                Ok(())
            }
            Key::Left if page == Page::Detail => {
                self.prose_dashboard.alternative =
                    self.prose_dashboard.alternative.saturating_sub(1);
                Ok(())
            }
            Key::Up => {
                self.prose_dashboard.selected = self.prose_dashboard.selected.saturating_sub(1);
                Ok(())
            }
            Key::Down => {
                let n = match page {
                    Page::Sources => self.prose_dashboard.sources.len(),
                    Page::Voice => self
                        .prose_dashboard
                        .voices
                        .as_ref()
                        .map_or(0, |s| s.value.profiles.len()),
                    _ => self.prose_dashboard.visible().len(),
                };
                self.prose_dashboard.selected =
                    (self.prose_dashboard.selected + 1).min(n.saturating_sub(1));
                Ok(())
            }
            Key::Home => {
                self.prose_dashboard.selected = 0;
                Ok(())
            }
            Key::End => {
                let n = match page {
                    Page::Sources => self.prose_dashboard.sources.len(),
                    Page::Voice => self
                        .prose_dashboard
                        .voices
                        .as_ref()
                        .map_or(0, |s| s.value.profiles.len()),
                    _ => self.prose_dashboard.visible().len(),
                };
                self.prose_dashboard.selected = n.saturating_sub(1);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.prose_dashboard.notice = format!("{e:#}");
        }
    }
}
fn bail_result(message: &str) -> Result<()> {
    bail!("{message}")
}
fn plain(s: &str, max: usize) -> String {
    s.chars()
        .take(max)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let d = &app.prose_dashboard;
    let options = app.prose_options();
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Prose and revision dashboard · local, advisory ")
        .border_style(Style::default().fg(theme.moon));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(inner);
    f.render_widget(
        Paragraph::new(format!(
            "{:?} · {:?} fiction · relaxed dialogue {} · footnotes {}\n{}\n{}",
            options.analysis.profile,
            options.preset,
            options.analysis.relaxed_dialogue,
            options.analysis.include_footnotes,
            plain(&d.notice, 2048),
            plain(&d.warning, 2048)
        ))
        .style(Style::default().fg(theme.dim)),
        header,
    );
    let mut rows: Vec<String> = vec![];
    let mut selected = None;
    match d.page {
        Page::Summary => {
            if d.pending() {
                rows.push("Running locally: at most one active job and one newest request; old/foreign results cannot be published.".into());
            }
            rows.push("r runs native patterns + spelling + proselint. d offers ONE Nabokov model run. Nothing runs merely on opening this reader.".into());
            rows.push("c clears cached prose results/underlines and stops pending work. Esc closes; completed current results can underline the original editor until any edit/policy/owner changes.".into());
            if let Some(result) = app.current_prose_analysis() {
                let m = &result.native.metrics;
                rows.push(format!(
                    "Source: {} · native {:?} · {} ms",
                    result.native.snapshot.stamp.document,
                    result.native.status,
                    result.native.elapsed_ms
                ));
                rows.push(format!("Analysis words: {} (not ledger/native goal word units); distinct normalized: {}",m.analysis_words,m.unique_normalized_words));
                rows.push(format!(
                    "Heuristic sentence segments: {}; mean {:.1} words; standard deviation {:.1}",
                    m.sentence_segments,
                    m.mean_sentence_words().unwrap_or(0.0),
                    m.sentence_standard_deviation().unwrap_or(0.0)
                ));
                rows.push(format!("Paragraphs: {}; dialogue words (heuristic): {}; commas / semicolons / dashes: {} / {} / {}",m.paragraph_segments,m.dialogue_words_heuristic,m.commas,m.semicolons,m.dashes));
                rows.push(match m.approximate_english_grade {Some(n)=>format!("Approximate English grade: {n:.1}. Heuristic syllables, not literary quality, reading ability or authorship evidence."),None=>"Approximate English grade unavailable: need 100 analysis words / 3 segments. Counts remain descriptive, not a score.".into()});
                rows.push(format!(
                    "Independent spelling: {} possible dictionary words{}; Harper: {}",
                    result.spelling_count,
                    if result.spelling_count > diag::MAX_FINDINGS {
                        " (256 listed; not complete)"
                    } else {
                        ""
                    },
                    if result.harper_cached {
                        "validated same-owner/current-config cache"
                    } else {
                        "not run/current; g opens its separate permission/check reader"
                    }
                ));
                for service in &result.services {
                    rows.push(format!(
                        "{} {}: {:?}; {} ms / child peak {} KiB; {} omitted. {}",
                        service.engine.name(),
                        service.engine.version(),
                        service.status,
                        service.elapsed_ms,
                        service.peak_rss_kib,
                        service.omitted,
                        service.notice
                    ));
                }
                if !result
                    .services
                    .iter()
                    .any(|s| s.engine == tools::Engine::Nabokov)
                {
                    rows.push("Nabokov: not run; requires separate explicit d, then y for this exact snapshot. No live permission is saved.".into());
                }
                rows.push(format!("Ranked review items: {}; {} queue-cap omissions; {} native omissions. Missing/failed/excluded engines are not a clean bill of health.",result.queue.items.len(),result.queue.omitted,result.native.omitted));
            } else {
                rows.push("No current result. Existing spelling and Harper controls remain independent; source/policy changes invalidate this reader.".into());
            }
            rows.push("Enter explicitly reviews these statistics, then opens the queue. Long sentences, passives, echoes and adverbs may be intentional. No overall literary-quality score.".into());
        }
        Page::Queue => {
            rows.push(format!(
                "REVIEW QUEUE · {:?} filter · priority is workflow, not quality",
                d.filter
            ));
            if app.current_prose_analysis().is_some() {
                for (i, item) in d.visible().iter().enumerate() {
                    if i == d.selected {
                        selected = Some(rows.len());
                    }
                    rows.push(format!(
                        "{} {:?} · {} / {} · {:?} · {}",
                        if i == d.selected { "›" } else { " " },
                        item.priority,
                        item.finding.rule.engine,
                        item.finding.rule.rule,
                        item.state,
                        plain(&item.finding.message, 300)
                    ));
                }
            }
            if rows.len() == 1 {
                rows.push("No current items in this filter. No findings is not a claim that unavailable engines ran.".into());
            }
        }
        Page::Detail => {
            if app.current_prose_analysis().is_some()
                && let Some(item) = d.selected_item()
            {
                let finding = &item.finding;
                rows.push(format!(
                    "{:?} · {} / {} / {} · {:?}",
                    item.priority,
                    finding.rule.engine,
                    finding.rule.version,
                    finding.rule.rule,
                    finding.context
                ));
                rows.push(format!(
                    "Original-source UTF-8 bytes {}..{}; {:?}",
                    finding.span.start, finding.span.end, item.state
                ));
                rows.push(format!(
                    "Source selection preview: {:?}",
                    plain(&finding.expected, 512)
                ));
                rows.push(finding.message.clone());
                rows.push(format!(
                    "Provenance: {}",
                    item.provenance
                        .iter()
                        .map(|r| format!("{}:{}/{}", r.engine, r.version, r.rule))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if finding.replacements.is_empty() {
                    rows.push("No literal replacement. Guidance only: g selects the original for native editing; s opens native spelling at it.".into());
                }
                for (i, alternative) in finding.replacements.iter().enumerate() {
                    rows.push(format!(
                        "{} Literal alternative: {:?} · {}",
                        if i == d.alternative { "›" } else { " " },
                        alternative.text,
                        if alternative.source_safe {
                            "a explicitly applies ONE native Replace; Undo remains yours"
                        } else {
                            "manual only: crosses source markup"
                        }
                    ));
                }
                rows.push("i ignores this occurrence for this run; m mutes the primary rule in this novel; M asks to mute personally. w reviewed / x dismissed save references, never prose.".into());
            } else {
                rows.push("No current selected finding. r refreshes this revision.".into());
            }
        }
        Page::ModelConsent => {
            rows.push("OPTIONAL NABOKOV / SPACY TRAINED MODEL · ALLOW ONCE?".into());
            rows.push("This is neural/statistical language inference, not a non-AI engine. y permits ONE run on the current opted-in prose projection. Notes, code, private comments and footnotes follow the explicit scope controls.".into());
            rows.push("No runtime download, network manuscript request, persisted live permission, detector/authorship rules or literary-quality score. The local subprocess exits after this job; pause/cancel suppresses stale publication. 65,536-byte source budget, 30s child CPU / 60s wall budget.".into());
            rows.push("Proselint and ordinary spelling do not need this model. Harper has its own separate permission. Esc declines. y explicitly allows this exact source/configuration once.".into());
        }
        Page::Voice => {
            rows.push("FROZEN SELECTED-REFERENCE VOICE · DESCRIPTIVE, NOT AUTHORSHIP PROOF".into());
            rows.push("Need 5,000 analysis words, 40 segments and one source with 150 distinct words. n selects your own SAVED original manuscript sources; research/notes/copies are not silently sampled.".into());
            if let Some(prepared) = &d.prepared {
                rows.push(format!("READY, NOT SAVED: {:?} · {} analysis words · {} segments / {} sources. s saves numeric/reference metadata only.",prepared.label(),prepared.metrics().analysis_words,prepared.metrics().sentence_segments,prepared.references().len()));
            }
            if let Some(store) = &d.voices {
                for (i, profile) in store.value.profiles.iter().enumerate() {
                    if i == d.selected {
                        selected = Some(rows.len());
                    }
                    rows.push(format!(
                        "{} #{} {:?} · {} analysis words · {} sources{}",
                        if i == d.selected { "›" } else { " " },
                        profile.id,
                        profile.label,
                        profile.metrics.analysis_words,
                        profile.references.len(),
                        if store.value.active == Some(profile.id) {
                            " · ACTIVE"
                        } else {
                            ""
                        }
                    ));
                }
                if let Some(profile) = store
                    .value
                    .active
                    .and_then(|id| store.value.profiles.iter().find(|p| p.id == id))
                {
                    rows.push(format!("Reference is frozen at {}. Originals can change; this baseline never live-rebases or certifies a writer's identity.",profile.created_at));
                    if let Some(current) = app.current_prose_analysis() {
                        match voice::compare(profile, &current.native) {
                            Ok(changes) => {
                                rows.push(format!("Current/reference informational feature deviations: {}. No total score; no automatic correction.",changes.len()));
                                for change in changes.iter().take(12) {
                                    rows.push(format!(
                                        "{}: reference {:.2}, current {:.2}. {}",
                                        change.feature,
                                        change.reference,
                                        change.current,
                                        change.explanation
                                    ));
                                }
                            }
                            Err(e) => rows.push(format!("Comparison unavailable: {e}")),
                        }
                    }
                }
            } else {
                rows.push("Voice metadata unavailable/corrupt/future. Original retained; no fallback overwrite.".into());
            }
            rows.push("c explicitly reviews the current numeric comparison. Enter selects an existing baseline; D explicitly asks to remove only its metadata. More/less than a reference can reflect narrator, form, genre and intention.".into());
        }
        Page::Sources => {
            rows.push(format!(
                "SELECT OWN SAVED ORIGINALS · {} chosen; max 64 · Space toggles; Enter names",
                d.chosen.len()
            ));
            for (pos, i) in d.sources.iter().enumerate() {
                if let Some(node) = app.project.nodes.get(*i) {
                    if pos == d.selected {
                        selected = Some(rows.len());
                    }
                    rows.push(format!(
                        "{} [{}] {}{}",
                        if pos == d.selected { "›" } else { " " },
                        if d.chosen.contains(i) { "x" } else { " " },
                        node.path
                            .strip_prefix(&app.project.root)
                            .unwrap_or(&node.path)
                            .display(),
                        if node.dirty {
                            " · UNSAVED, save first"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        Page::VoiceName => {
            rows.push(format!("Frozen reference label: {}▏", d.name));
            rows.push("Enter prepares statistics off-thread; it does NOT save. Review material adequacy, then s saves. Delete clears; pasted text is a label, never commands.".into());
        }
        Page::DeleteVoice => {
            rows.push(format!("Remove frozen voice metadata {:?}? Originals and all manuscript files remain untouched. y confirms; Esc retains it.",d.pending_profile));
        }
        Page::GlobalMute => {
            rows.push(format!("Personally mute {:?}? y saves ONLY a version-bound rule identifier, not model permission. It applies to future matching findings across novels using this personal profile. Esc declines.",d.pending_rule));
        }
    }
    // Static wrapped text. No parser, disk read, worker dispatch or network here.
    let width = body.width.max(1) as usize;
    let mut lines = vec![];
    let mut selected_line = None;
    for (i, row) in rows.iter().enumerate() {
        if selected == Some(i) {
            selected_line = Some(lines.len());
        }
        for line in crate::ui::wrap_words(&plain(row, 8192), width) {
            lines.push(Line::raw(line));
        }
    }
    let max = lines.len().saturating_sub(body.height as usize);
    let auto = selected_line
        .map_or(0, |row| row.saturating_sub(body.height as usize / 2))
        .min(max);
    let at = d.scroll.unwrap_or(auto).min(max);
    d.fit.set((at, max));
    f.render_widget(
        Paragraph::new(lines)
            .scroll((at.min(u16::MAX as usize) as u16, 0))
            .style(Style::default().fg(theme.text)),
        body,
    );
    f.render_widget(Paragraph::new("r run · d model once? · p profile · f fiction · l dialogue · o footnotes · v voice\nEnter inspect · Tab filter · a apply one · g original/Harper · w reviewed · x dismiss\n↑↓ select · PgUp/PgDn scroll · Esc back/close · F1 offline help").style(Style::default().fg(theme.dim)),footer);
}
