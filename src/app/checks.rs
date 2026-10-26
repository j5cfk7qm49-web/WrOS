//! Opt-in, source-bound writing checks. Snapshots are read-only copies of the
//! existing exclusive editors; this module owns no editor and no save path.
use super::*;
use anyhow::{bail, ensure};
use grimoire_core::{
    diagnostics::{
        self as model, AnalysisOptions, Diagnostic, SourceRole, SourceUnit,
        harper::{Report, ReportStatus},
        preferences::{CONSENT_VERSION, NovelPreferences, PersonalPreferences},
        worker::Worker,
    },
    statefile::Store,
};
use ratatui::{
    Frame,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::collections::{BTreeMap, BTreeSet};

const DEBOUNCE_MS: u64 = 900;
const LOW_POWER_DEBOUNCE_MS: u64 = 2_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Findings,
    Settings,
    ConsentOnce,
    ConsentLive,
    MuteGlobal,
    ClearNovel,
    ClearGlobal,
    RevokeModel,
}
struct Slot {
    snapshot: model::Snapshot,
    configuration: String,
    changed_ms: u64,
    pending: bool,
    once_requested: bool,
    report: Option<Report>,
}
pub struct Checks {
    pub novel: Option<Store<NovelPreferences>>,
    pub personal: Option<Store<PersonalPreferences>>,
    pub warning: String,
    pub notice: String,
    pub selected: usize,
    pub alternative: usize,
    pub scroll: Option<usize>,
    pub render_fit: std::cell::Cell<(usize, usize)>,
    pub worker: Option<Worker>,
    slots: Vec<Slot>,
    once: BTreeSet<String>,
    personal_root: Option<PathBuf>,
    session_consent: bool,
    session_paused: bool,
    generation: u64,
    last_scan_ms: Option<u64>,
    last_work_ms: u64,
    pub discarded: u64,
    pending_rule: Option<model::RuleKey>,
    project_binding: Option<String>,
}
impl Checks {
    pub fn load(root: &Path, personal_root: Option<PathBuf>) -> Self {
        let (novel, local_warning) = match NovelPreferences::load(root) {
            Ok(store) => (Some(store), String::new()),
            Err(e) => (
                None,
                format!("Novel check settings unavailable: {e:#}; originals preserved. "),
            ),
        };
        let (personal, personal_warning) = match personal_root
            .as_deref()
            .map(PersonalPreferences::load)
            .transpose()
        {
            Ok(store) => (store, String::new()),
            Err(e) => (
                None,
                format!("Personal check settings unavailable: {e:#}; originals preserved."),
            ),
        };
        let session_consent = personal.as_ref().is_some_and(|s| s.value.allows_runtime());
        Self {
            novel,
            personal,
            personal_root,
            warning: local_warning + &personal_warning,
            notice: String::new(),
            selected: 0,
            alternative: 0,
            scroll: None,
            render_fit: std::cell::Cell::new((0, 0)),
            worker: None,
            slots: vec![],
            once: BTreeSet::new(),
            session_consent,
            session_paused: false,
            generation: 0,
            last_scan_ms: None,
            last_work_ms: 0,
            discarded: 0,
            pending_rule: None,
            project_binding: model::preferences::project_binding(root).ok(),
        }
    }
    pub fn live(&self) -> bool {
        !self.session_paused
            && self.novel.as_ref().is_some_and(|s| s.value.live_enabled())
            && self.personal.as_ref().is_some_and(|s| {
                s.value.allows_runtime()
                    && self
                        .project_binding
                        .as_ref()
                        .is_some_and(|binding| s.value.live_projects.contains(binding))
            })
    }
    pub fn options(&self) -> AnalysisOptions {
        let defaults = PersonalPreferences::default();
        let novel_default = NovelPreferences::default();
        self.novel
            .as_ref()
            .map_or(&novel_default, |s| &s.value)
            .effective(self.personal.as_ref().map_or(&defaults, |p| &p.value))
    }
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }
    fn invalidate(&mut self) {
        if let Some(worker) = &self.worker {
            worker.clear();
        }
        self.slots.clear();
        self.once.clear();
        self.scroll = None;
        self.selected = 0;
        self.alternative = 0;
        self.last_scan_ms = None;
    }
    fn pause(&mut self) {
        self.session_paused = true;
        self.worker = None;
        self.invalidate();
    }
    #[cfg(test)]
    pub fn use_personal_root(&mut self, root: &Path) {
        self.personal_root = Some(root.into());
        self.personal = Some(PersonalPreferences::load(root).unwrap());
        self.session_consent = self.personal.as_ref().unwrap().value.allows_runtime();
    }
}
impl App {
    fn check_role(&self, i: usize) -> SourceRole {
        let n = &self.project.nodes[i];
        if n.in_manuscript && !self.project.in_trash(i) {
            SourceRole::Manuscript
        } else if n
            .path
            .strip_prefix(&self.project.root)
            .is_ok_and(|p| p.starts_with("research"))
        {
            SourceRole::Research
        } else {
            SourceRole::Note
        }
    }
    fn check_document(&self, pane: Pane) -> Result<(usize, String)> {
        if pane == Pane::Secondary && (!self.views.secondary_visible || !self.views.secondary_room)
        {
            bail!("secondary source is hidden; no check or edit routed there");
        }
        let i = self
            .view_open(pane)
            .ok_or_else(|| anyhow::anyhow!("open a scene in an editable view first"))?;
        let node = &self.project.nodes[i];
        ensure!(
            node.kind == Kind::Scene
                && !node.read_only
                && !node.parked
                && !self.project.in_trash(i)
                && !node.never_read(),
            "source is read-only, parked, trashed or unavailable; no check or edit"
        );
        ensure!(
            node.disk.unavailable.is_none() && node.disk.pending.is_none(),
            "source has an unresolved disk change; review it first"
        );
        let document = node
            .path
            .strip_prefix(&self.project.root)?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("source reference is not UTF-8"))?
            .to_owned();
        Ok((i, document))
    }
    fn check_snapshot(&self, pane: Pane, generation: u64) -> Result<model::Snapshot> {
        let (i, document) = self.check_document(pane)?;
        let editor = self
            .view_editor(pane)
            .ok_or_else(|| anyhow::anyhow!("source view unavailable"))?;
        let bytes = editor
            .lines
            .iter()
            .map(String::len)
            .sum::<usize>()
            .saturating_add(editor.lines.len().saturating_sub(1));
        ensure!(
            bytes <= model::MAX_SOURCE_BYTES,
            "source exceeds {} byte snapshot cap; nothing analyzed",
            model::MAX_SOURCE_BYTES
        );
        model::Snapshot::new(
            Path::new(&document),
            SourceUnit::EditorBody,
            self.check_role(i),
            generation,
            editor.text(),
        )
    }
    pub fn open_checks(&mut self) {
        self.overlay = Overlay::Checks {
            page: Page::Findings,
        };
        self.checks.selected = 0;
        self.checks.alternative = 0;
        self.checks.scroll = None;
        self.tick_checks();
    }
    /// Existing injected monotonic clock; no activity/typing credit is derived
    /// from this debounce, a running analyzer, or an open inspector.
    pub fn tick_checks(&mut self) {
        let now = self.productivity.clock.now().mono_ms;
        self.tick_checks_at(now);
    }
    pub(crate) fn tick_checks_at(&mut self, now: u64) {
        let live = self.checks.live();
        let inspecting = matches!(self.overlay, Overlay::Checks { .. });
        if !live
            && now.saturating_sub(self.checks.last_work_ms) >= 30_000
            && self.checks.slots.iter().all(|s| !s.pending)
        {
            self.checks.worker = None;
        }
        if !live
            && !inspecting
            && self
                .checks
                .slots
                .iter()
                .all(|s| !s.pending && s.report.is_none())
        {
            self.checks.slots.clear();
        }
        if !live && !inspecting && self.checks.slots.is_empty() {
            return;
        }
        let options = self.checks.options();
        let configuration = options.digest();
        let scan = self
            .checks
            .last_scan_ms
            .is_none_or(|last| now.saturating_sub(last) >= 100);
        if scan {
            self.checks.last_scan_ms = Some(now);
            let mut present = BTreeSet::new();
            for pane in [Pane::Primary, Pane::Secondary] {
                let Ok((i, document)) = self.check_document(pane) else {
                    continue;
                };
                let Some(editor) = self.view_editor(pane) else {
                    continue;
                };
                let size = editor
                    .lines
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(editor.lines.len().saturating_sub(1));
                if size > model::MAX_SOURCE_BYTES {
                    continue;
                }
                let text = editor.text();
                let hash = model::hex_digest(text.as_bytes());
                let role = self.check_role(i);
                present.insert(document.clone());
                let index = self
                    .checks
                    .slots
                    .iter()
                    .position(|s| s.snapshot.stamp.document == document);
                if let Some(index) = index {
                    let slot = &self.checks.slots[index];
                    if slot.snapshot.stamp.sha256 == hash
                        && slot.snapshot.stamp.role == role
                        && slot.configuration == configuration
                    {
                        continue;
                    }
                    if let Some(worker) = &self.checks.worker {
                        worker.cancel(&document, SourceUnit::EditorBody);
                    }
                    let generation = self.checks.next_generation();
                    if let Ok(snapshot) = model::Snapshot::new(
                        Path::new(&document),
                        SourceUnit::EditorBody,
                        role,
                        generation,
                        text,
                    ) {
                        self.checks.slots[index] = Slot {
                            snapshot,
                            configuration: configuration.clone(),
                            changed_ms: now,
                            pending: false,
                            once_requested: false,
                            report: None,
                        };
                    }
                } else {
                    let generation = self.checks.next_generation();
                    if let Ok(snapshot) = model::Snapshot::new(
                        Path::new(&document),
                        SourceUnit::EditorBody,
                        role,
                        generation,
                        text,
                    ) {
                        self.checks.slots.push(Slot {
                            snapshot,
                            configuration: configuration.clone(),
                            changed_ms: now,
                            pending: false,
                            once_requested: false,
                            report: None,
                        });
                    }
                }
            }
            let removed: Vec<_> = self
                .checks
                .slots
                .iter()
                .filter(|s| !present.contains(&s.snapshot.stamp.document))
                .map(|s| s.snapshot.stamp.document.clone())
                .collect();
            for doc in removed {
                if let Some(worker) = &self.checks.worker {
                    worker.cancel(&doc, SourceUnit::EditorBody);
                }
            }
            self.checks
                .slots
                .retain(|s| present.contains(&s.snapshot.stamp.document));
            self.checks.slots.truncate(2);
            let current_ids: BTreeSet<_> = self
                .checks
                .slots
                .iter()
                .filter_map(|s| s.report.as_ref())
                .flat_map(|r| r.findings.iter().map(|d| d.id.clone()))
                .collect();
            self.checks.once.retain(|id| current_ids.contains(id));
        }
        if let Some(worker) = &self.checks.worker {
            loop {
                match worker.try_recv() {
                    Ok(Some(report)) => {
                        let slot = self
                            .checks
                            .slots
                            .iter_mut()
                            .find(|s| report.matches(&s.snapshot, &options));
                        if let Some(slot) = slot {
                            slot.pending = false;
                            slot.once_requested = false;
                            slot.report = Some(report);
                            self.checks.last_work_ms = now;
                        } else {
                            self.checks.discarded = self.checks.discarded.saturating_add(1);
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        self.checks.notice = e.to_string();
                        for slot in &mut self.checks.slots {
                            if slot.pending {
                                slot.pending = false;
                                slot.once_requested = false;
                                slot.report = Some(Report::empty(
                                    slot.snapshot.clone(),
                                    options.clone(),
                                    ReportStatus::EngineFailed(
                                        "worker unavailable; r explicitly retries".into(),
                                    ),
                                ));
                            }
                        }
                        self.checks.worker = None;
                        break;
                    }
                }
            }
        }
        let delay = if self.deck.state.low_power {
            LOW_POWER_DEBOUNCE_MS
        } else {
            DEBOUNCE_MS
        };
        let ready = self.checks.slots.iter().any(|s| {
            !s.pending
                && s.report.is_none()
                && (s.once_requested || (live && now.saturating_sub(s.changed_ms) >= delay))
        });
        if ready && self.checks.session_consent {
            if self.checks.worker.is_none() {
                match Worker::start() {
                    Ok(worker) => self.checks.worker = Some(worker),
                    Err(e) => {
                        self.checks.notice =
                            format!("cannot start local analyzer: {e:#}; r retries");
                        for slot in &mut self.checks.slots {
                            slot.pending = false;
                            slot.once_requested = false;
                            slot.report = Some(Report::empty(
                                slot.snapshot.clone(),
                                options.clone(),
                                ReportStatus::EngineFailed(
                                    "thread start failed; r explicitly retries".into(),
                                ),
                            ));
                        }
                        return;
                    }
                }
            }
            let worker = self.checks.worker.as_ref().unwrap();
            for slot in &mut self.checks.slots {
                if !slot.pending
                    && slot.report.is_none()
                    && (slot.once_requested
                        || (live && now.saturating_sub(slot.changed_ms) >= delay))
                {
                    match worker.submit(slot.snapshot.clone(), options.clone()) {
                        Ok(_) => {
                            slot.pending = true;
                            slot.once_requested = false;
                            self.checks.last_work_ms = now;
                        }
                        Err(e) => {
                            self.checks.notice = e.to_string();
                            slot.once_requested = false;
                        }
                    }
                }
            }
        }
    }
    #[cfg(test)]
    pub fn install_check_report_for_test(&mut self, report: Report) {
        assert_eq!(report.snapshot.text(), self.editor.text());
        assert_eq!(report.options, self.checks.options());
        self.checks.generation = self.checks.generation.max(report.snapshot.stamp.generation);
        self.checks.slots = vec![Slot {
            snapshot: report.snapshot.clone(),
            configuration: report.options.digest(),
            changed_ms: 0,
            pending: false,
            once_requested: false,
            report: Some(report),
        }];
        self.checks.selected = 0;
    }
    fn request_check_once(&mut self) -> Result<()> {
        self.check_document(self.views.active)?;
        if !self.checks.session_consent {
            self.overlay = Overlay::Checks {
                page: Page::ConsentOnce,
            };
            return Ok(());
        }
        let generation = self.checks.next_generation();
        let snapshot = self.check_snapshot(self.views.active, generation)?;
        let now = self.productivity.clock.now().mono_ms;
        let configuration = self.checks.options().digest();
        let document = snapshot.stamp.document.clone();
        if let Some(worker) = &self.checks.worker {
            worker.cancel(&document, SourceUnit::EditorBody);
        }
        let visible: BTreeSet<_> = [Pane::Primary, Pane::Secondary]
            .into_iter()
            .filter_map(|p| self.check_document(p).ok().map(|(_, d)| d))
            .collect();
        self.checks.slots.retain(|s| {
            s.snapshot.stamp.document != document && visible.contains(&s.snapshot.stamp.document)
        });
        self.checks.slots.push(Slot {
            snapshot,
            configuration,
            changed_ms: now,
            pending: false,
            once_requested: true,
            report: None,
        });
        self.checks.once.clear();
        self.checks.selected = 0;
        self.checks.alternative = 0;
        self.checks.last_scan_ms = Some(now);
        self.checks.notice =
            "Local check requested; no prose or result is saved as metadata.".into();
        self.tick_checks_at(now);
        Ok(())
    }
    /// Validation here also runs immediately during draw/actions, not just on
    /// the 100 ms reconciliation tick. A changed buffer never flashes old marks.
    fn current_check_report(&self) -> Option<&Report> {
        let (i, document) = self.check_document(self.views.active).ok()?;
        let slot = self
            .checks
            .slots
            .iter()
            .find(|s| s.snapshot.stamp.document == document)?;
        let report = slot.report.as_ref()?;
        if report.snapshot.stamp != slot.snapshot.stamp
            || report.options != self.checks.options()
            || report.snapshot.stamp.unit != SourceUnit::EditorBody
            || report.snapshot.stamp.role != self.check_role(i)
        {
            return None;
        }
        let size = self
            .editor
            .lines
            .iter()
            .map(String::len)
            .sum::<usize>()
            .saturating_add(self.editor.lines.len().saturating_sub(1));
        if size != report.snapshot.text().len()
            || model::hex_digest(self.editor.text().as_bytes()) != report.snapshot.stamp.sha256
        {
            return None;
        }
        Some(report)
    }
    fn visible_checks(&self) -> Vec<&Diagnostic> {
        self.current_check_report()
            .into_iter()
            .flat_map(|r| &r.findings)
            .filter(|d| !self.checks.once.contains(&d.id))
            .collect()
    }
    pub(crate) fn selected_check(&self) -> Result<Diagnostic> {
        let findings = self.visible_checks();
        findings
            .get(self.checks.selected.min(findings.len().saturating_sub(1)))
            .map(|d| (*d).clone())
            .ok_or_else(|| anyhow::anyhow!("no current finding; r checks this revision"))
    }
    pub(crate) fn apply_check_finding(
        &mut self,
        finding: &Diagnostic,
        alternative: usize,
    ) -> Result<()> {
        self.ensure_document_claims()?;
        let (i, document) = self.check_document(self.views.active)?;
        let generation = self
            .checks
            .slots
            .iter()
            .find(|s| s.snapshot.stamp.document == document)
            .ok_or_else(|| anyhow::anyhow!("no current check generation"))?
            .snapshot
            .stamp
            .generation;
        let current = self.check_snapshot(self.views.active, generation)?;
        let options = self.checks.options();
        let (span, replacement) = finding.checked_replacement(&current, &options, alternative)?;
        let replacement = replacement.to_owned();
        // Check the actual backing file before changing the existing buffer.
        // Ordinary autosave still uses its established conflict/recovery guard.
        let node = &self.project.nodes[i];
        ensure!(
            node.seen.is_some() && grimoire_core::sync::Fingerprint::read(&node.path) == node.seen,
            "backing source changed or is unreadable; refresh/reconcile it before applying"
        );
        let start = current
            .point(span.start)
            .ok_or_else(|| anyhow::anyhow!("invalid source start"))?;
        let end = current
            .point(span.end)
            .ok_or_else(|| anyhow::anyhow!("invalid source end"))?;
        ensure!(
            current.slice(span) != Some(replacement.as_str()),
            "replacement would not change source"
        );
        self.editor.select(start, end);
        self.editor
            .replace_selection_with_as(&replacement, grimoire_core::measure::Origin::Replace);
        self.focus = self.editor_focus();
        self.flush();
        if finding.category == model::Category::Grammar
            && self.check_role(i) == SourceRole::Manuscript
        {
            self.observe_deck(grimoire_core::tutorial::Event::GrammarApplied);
        }
        self.checks.invalidate();
        self.overlay = Overlay::None;
        self.msg = "Applied reviewed source edit — Ctrl-Z undoes it; not typing credit.".into();
        Ok(())
    }
    fn select_check_source(&mut self, finding: &Diagnostic) -> Result<()> {
        let report = self
            .current_check_report()
            .ok_or_else(|| anyhow::anyhow!("stale check; run this revision again"))?;
        ensure!(
            report.snapshot.stamp == finding.source,
            "finding belongs to another source/revision"
        );
        let start = report
            .snapshot
            .point(finding.span.start)
            .ok_or_else(|| anyhow::anyhow!("invalid finding start"))?;
        let end = report
            .snapshot
            .point(finding.span.end)
            .ok_or_else(|| anyhow::anyhow!("invalid finding end"))?;
        self.editor.select(start, end);
        self.focus = self.editor_focus();
        self.overlay = Overlay::None;
        self.msg = "Selected the original source; no suggestion applied.".into();
        Ok(())
    }
    /// Existing render layers use scalar columns, never UTF-8 bytes/display
    /// cells. This is computed once per prose view, not once per painted row.
    pub fn check_line_spans(&self) -> BTreeMap<usize, Vec<(usize, usize)>> {
        let mut out: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        let Some(report) = self.current_check_report() else {
            return out;
        };
        if report.status != ReportStatus::Complete {
            return out;
        }
        for finding in &report.findings {
            if self.checks.once.contains(&finding.id) {
                continue;
            }
            let (Some((line, col)), Some((end_line, end_col))) = (
                report.snapshot.point(finding.span.start),
                report.snapshot.point(finding.span.end),
            ) else {
                continue;
            };
            for row in line..=end_line {
                let from = if row == line { col } else { 0 };
                let to = if row == end_line {
                    end_col
                } else {
                    self.editor.lines.get(row).map_or(0, |s| s.chars().count())
                };
                if from < to {
                    out.entry(row).or_default().push((from, to));
                }
            }
        }
        out
    }
    fn save_check_options(&mut self, f: impl FnOnce(&mut AnalysisOptions)) -> Result<()> {
        self.checks
            .novel
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("novel check settings unavailable; original preserved"))?
            .update(|p| {
                f(&mut p.options);
                Ok(())
            })?;
        self.checks.invalidate();
        self.checks.notice =
            "Check settings saved; r checks once, or live checks wait for idle.".into();
        Ok(())
    }
    fn enable_live_checks(&mut self) -> Result<()> {
        ensure!(
            self.checks.novel.is_some() && self.checks.personal.is_some(),
            "novel/personal check settings unavailable; no live consent written"
        );
        let binding = self.checks.project_binding.clone().ok_or_else(|| {
            anyhow::anyhow!("project location cannot be bound; live checks not enabled")
        })?;
        let personal = self.checks.personal.as_mut().unwrap();
        if !personal.value.allows_runtime() || !personal.value.live_projects.contains(&binding) {
            personal.update(|p| {
                p.allowed_runtime = Some(CONSENT_VERSION.into());
                p.live_projects.insert(binding);
                Ok(())
            })?;
        }
        // Two separately guarded records, not a cross-file atomic transaction.
        // If the second write fails, personal permission may be remembered,
        // but live analysis cannot start without the novel's consent as well.
        self.checks.novel.as_mut().unwrap().update(|p| {
            p.live_consent = Some(CONSENT_VERSION.into());
            Ok(())
        })?;
        self.checks.session_consent = true;
        self.checks.session_paused = false;
        self.checks.invalidate();
        self.checks.notice = "Live local checks enabled for this novel; 0.9 s idle (2 s low power). Spelling remains independent.".into();
        Ok(())
    }
    fn disable_live_checks(&mut self) -> Result<()> {
        self.checks.pause(); // Stop immediately even if either guarded write fails.
        let binding = self.checks.project_binding.clone();
        let personal = self
            .checks
            .personal
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("personal settings unavailable"))
            .and_then(|store| {
                store.update(|p| {
                    if let Some(binding) = binding {
                        p.live_projects.remove(&binding);
                    }
                    Ok(())
                })
            });
        let novel = self
            .checks
            .novel
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("novel settings unavailable"))
            .and_then(|store| {
                store.update(|p| {
                    p.live_consent = None;
                    Ok(())
                })
            });
        if personal.is_err() || novel.is_err() {
            bail!(
                "stopped for session; profile binding revoked: {}; novel off persisted: {}. Originals preserved. {} {}",
                personal.is_ok(),
                novel.is_ok(),
                personal.err().map_or(String::new(), |e| e.to_string()),
                novel.err().map_or(String::new(), |e| e.to_string())
            );
        }
        self.checks.notice =
            "Live checks off and analyzer stopped. Existing spelling is unchanged.".into();
        Ok(())
    }
    fn mute_check_rule(&mut self, global: bool, rule: model::RuleKey) -> Result<()> {
        if global {
            self.checks
                .personal
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("personal profile unavailable; nothing muted"))?
                .update(|p| {
                    p.muted_rules.insert(rule);
                    Ok(())
                })?;
            self.checks.invalidate();
            self.checks.notice =
                "Rule muted in this personal profile, version-bound; r refreshes. No prose stored."
                    .into();
        } else {
            self.save_check_options(|p| {
                p.disabled_rules.insert(rule);
            })?;
        }
        Ok(())
    }
    fn reload_checks(&mut self) {
        let paused = self.checks.session_paused;
        let personal = self.checks.personal_root.clone();
        self.checks = Checks::load(&self.project.root, personal);
        self.checks.session_paused = paused;
        self.checks.notice =
            "Settings explicitly reloaded; corrupt/future originals are not reset.".into();
    }
    pub fn on_checks_key(&mut self, key: Key) {
        let Overlay::Checks { page } = self.overlay else {
            return;
        };
        if page != Page::Findings
            && matches!(key, Key::PageDown | Key::PageUp | Key::Home | Key::End)
        {
            let (offset, max) = self.checks.render_fit.get();
            self.checks.scroll = Some(match key {
                Key::PageDown => offset.saturating_add(10).min(max),
                Key::PageUp => offset.saturating_sub(10),
                Key::Home => 0,
                _ => max,
            });
            return;
        }
        self.checks.scroll = None;
        let outcome: Result<()> = (|| {
            match page {
                Page::ConsentOnce | Page::ConsentLive => match key {
                    Key::Char('y') => {
                        self.overlay = Overlay::Checks {
                            page: Page::Findings,
                        };
                        if page == Page::ConsentLive {
                            self.enable_live_checks()?;
                        } else {
                            self.checks.session_consent = true;
                            self.request_check_once()?;
                        }
                    }
                    Key::Esc | Key::Char('n' | 'q') => {
                        self.overlay = Overlay::Checks {
                            page: Page::Findings,
                        }
                    }
                    _ => {}
                },
                Page::MuteGlobal | Page::ClearNovel | Page::ClearGlobal | Page::RevokeModel => {
                    match key {
                        Key::Char('y') => {
                            if page == Page::RevokeModel {
                                self.checks.pause();
                                self.checks.session_consent = false;
                                self.checks.personal.as_mut().ok_or_else(|| anyhow::anyhow!("personal settings unavailable; analyzer stopped for this session"))?.update(|p| { p.allowed_runtime = None; p.live_projects.clear(); Ok(()) })?;
                                self.checks.notice = "Personal model permission and all location bindings revoked; this process stopped. Other already-running instances require reload or exit. Spelling is unchanged.".into();
                            } else if page == Page::MuteGlobal {
                                let rule = self
                                    .checks
                                    .pending_rule
                                    .take()
                                    .ok_or_else(|| anyhow::anyhow!("no selected rule"))?;
                                self.mute_check_rule(true, rule)?;
                            } else if page == Page::ClearNovel {
                                self.save_check_options(|p| p.disabled_rules.clear())?;
                            } else {
                                self.checks
                                    .personal
                                    .as_mut()
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("personal preferences unavailable")
                                    })?
                                    .update(|p| {
                                        p.muted_rules.clear();
                                        Ok(())
                                    })?;
                                self.checks.invalidate();
                                self.checks.notice =
                                    "Personal rule mutes cleared; r refreshes.".into();
                            }
                            self.overlay = Overlay::Checks {
                                page: Page::Findings,
                            };
                        }
                        Key::Esc | Key::Char('n' | 'q') => {
                            self.checks.pending_rule = None;
                            self.overlay = Overlay::Checks {
                                page: Page::Findings,
                            };
                        }
                        _ => {}
                    }
                }
                Page::Settings => match key {
                    Key::Char('k') => {
                        self.overlay = Overlay::Checks {
                            page: Page::RevokeModel,
                        }
                    }
                    Key::Esc | Key::Char('s' | 'q') => {
                        self.overlay = Overlay::Checks {
                            page: Page::Findings,
                        }
                    }
                    Key::Char('p') => self.save_check_options(|p| {
                        p.profile = match p.profile {
                            model::Profile::Drafting => model::Profile::Revision,
                            model::Profile::Revision => model::Profile::DeepReview,
                            model::Profile::DeepReview => model::Profile::Drafting,
                        }
                    })?,
                    Key::Char('d') => self.save_check_options(|p| {
                        p.dialect = match p.dialect {
                            model::Dialect::American => model::Dialect::British,
                            model::Dialect::British => model::Dialect::Australian,
                            model::Dialect::Australian => model::Dialect::Canadian,
                            model::Dialect::Canadian => model::Dialect::American,
                        }
                    })?,
                    Key::Char('f') => {
                        self.save_check_options(|p| p.include_footnotes = !p.include_footnotes)?
                    }
                    Key::Char('n') => {
                        self.save_check_options(|p| p.include_notes = !p.include_notes)?
                    }
                    Key::Char('r') => {
                        self.save_check_options(|p| p.include_research = !p.include_research)?
                    }
                    Key::Char('v') => {
                        self.save_check_options(|p| p.relaxed_dialogue = !p.relaxed_dialogue)?
                    }
                    Key::Char('x') => {
                        self.overlay = Overlay::Checks {
                            page: Page::ClearNovel,
                        }
                    }
                    Key::Char('X') => {
                        self.overlay = Overlay::Checks {
                            page: Page::ClearGlobal,
                        }
                    }
                    Key::Char('u') => self.reload_checks(),
                    _ => {}
                },
                Page::Findings => match key {
                    Key::Esc | Key::Char('q') => self.overlay = Overlay::None,
                    Key::Char('r') => self.request_check_once()?,
                    Key::Char('l') => {
                        if self.checks.live() {
                            self.disable_live_checks()?;
                        } else {
                            self.overlay = Overlay::Checks {
                                page: Page::ConsentLive,
                            };
                        }
                    }
                    Key::Char('c') => {
                        self.checks.pause();
                        self.checks.notice = "Cleared results; checks paused for this session. Saved live preference is unchanged.".into();
                    }
                    Key::Char('s') => {
                        self.overlay = Overlay::Checks {
                            page: Page::Settings,
                        }
                    }
                    Key::Char('u') => self.reload_checks(),
                    Key::Down | Key::Char('j') | Key::PageDown => {
                        let len = self.visible_checks().len();
                        let step = if key == Key::PageDown { 10 } else { 1 };
                        self.checks.selected = self
                            .checks
                            .selected
                            .saturating_add(step)
                            .min(len.saturating_sub(1));
                        self.checks.alternative = 0;
                    }
                    Key::Up | Key::Char('k') | Key::PageUp => {
                        let step = if key == Key::PageUp { 10 } else { 1 };
                        self.checks.selected = self.checks.selected.saturating_sub(step);
                        self.checks.alternative = 0;
                    }
                    Key::Home => {
                        self.checks.selected = 0;
                        self.checks.alternative = 0;
                    }
                    Key::End => {
                        self.checks.selected = self.visible_checks().len().saturating_sub(1);
                        self.checks.alternative = 0;
                    }
                    Key::Tab | Key::Right => {
                        let finding = self.selected_check()?;
                        self.checks.alternative =
                            (self.checks.alternative + 1) % finding.replacements.len().max(1);
                    }
                    Key::Enter => {
                        let finding = self.selected_check()?;
                        self.select_check_source(&finding)?;
                    }
                    Key::Char('a') => {
                        let finding = self.selected_check()?;
                        self.apply_check_finding(&finding, self.checks.alternative)?;
                    }
                    Key::Char('i') => {
                        let finding = self.selected_check()?;
                        self.checks.once.insert(finding.id);
                        self.checks.selected = self
                            .checks
                            .selected
                            .min(self.visible_checks().len().saturating_sub(1));
                        self.checks.notice =
                            "Ignored this finding for this pass only; no file written.".into();
                    }
                    Key::Char('n') => {
                        let finding = self.selected_check()?;
                        self.mute_check_rule(false, finding.rule)?;
                    }
                    Key::Char('G') => {
                        self.checks.pending_rule = Some(self.selected_check()?.rule);
                        self.overlay = Overlay::Checks {
                            page: Page::MuteGlobal,
                        };
                    }
                    _ => {}
                },
            }
            Ok(())
        })();
        if let Err(e) = outcome {
            self.checks.notice = format!("{e:#}");
        }
    }
    fn check_rows(&self, page: Page) -> (Vec<String>, Option<usize>) {
        let mut rows = vec![format!(
            "Harper {} · live {} · local only · no automatic changes",
            model::HARPER_VERSION,
            if self.checks.live() { "ON" } else { "OFF" }
        )];
        let mut selected = None;
        if !self.checks.warning.is_empty() {
            rows.push(self.checks.warning.clone());
        }
        match page {
            Page::ConsentOnce | Page::ConsentLive => {
                rows.extend([
                    "Optional local language analysis — explicit permission".into(),
                    "Harper uses a bundled part-of-speech tagger and neural chunker. It is not described as a non-AI engine. No network, account, model download or manuscript transmission is used.".into(),
                    "Spelling, editing, saving and all ordinary writing remain available without this service. Findings are fallible review suggestions, not a literary-quality score.".into(),
                    if page == Page::ConsentOnce { "y runs once and remembers permission only for this process; no settings/diagnostic prose is written." } else { "y remembers model permission and a location/runtime binding in your personal profile, plus live checks for THIS novel. These are two separately guarded writes, not an atomic cross-file transaction. A changed runtime requires new permission." }.into(),
                    "y permit · n/Esc cancel · nothing is accepted by merely opening this page".into(),
                ]);
            }
            Page::MuteGlobal => rows.push(format!("Mute rule {:?} in your personal profile, across novels? This is version-bound, not a phrase/source-text store. y confirms; n/Esc cancels.", self.checks.pending_rule)),
            Page::ClearNovel => rows.push("Clear only this novel's muted-rule identifiers? Personal mutes remain. y confirms; n/Esc cancels.".into()),
            Page::RevokeModel => rows.push("Revoke saved model permission and ALL personal project/location bindings? This process stops on confirmation; other already-running instances must reload/exit. Novel settings/mutes are preserved but cannot enable live work without renewed personal permission. y confirms; n/Esc cancels.".into()),
            Page::ClearGlobal => rows.push("Clear personal muted-rule identifiers across novels? Novel mutes and model permission remain. y confirms; n/Esc cancels.".into()),
            Page::Settings => {
                let o = self.checks.options();
                rows.extend([
                    format!("p Profile: {:?} (Harper only; Python/readability dashboard is separate work)", o.profile),
                    format!("d English dialect: {:?} (native spelling keeps its own setting)", o.dialect),
                    format!("f Footnote bodies: {} (labels are never prose)", o.include_footnotes),
                    format!("n Notes: {} · r Research: {}", o.include_notes, o.include_research),
                    format!("v Relax quoted dialogue style/capitalization: {}. Grammar is still checked. Attribution is a heuristic.", o.relaxed_dialogue),
                    format!("Novel rule mutes: {} · personal mutes: {}", self.checks.novel.as_ref().map_or(0, |s| s.value.options.disabled_rules.len()), self.checks.personal.as_ref().map_or(0, |s| s.value.muted_rules.len())),
                    "x clear novel mutes · X clear personal mutes · k revoke all personal model permissions (confirmation) · u reload".into(),
                    "Input uses the existing editor BODY byte space, not byte offsets in the disk file's YAML header. Whole-file adapters have a separate explicit coordinate space.".into(),
                    "Private %% notes %%, TK markers, code, link targets and metadata are excluded. Text/formatting is never stripped and guessed back into place. Suggestions crossing markup/entities need manual edits.".into(),
                    "At most two visible sources, one local worker; 65,536-byte Harper cap, 256 displayed findings. Larger sources are explicitly NOT checked, never silently truncated. Spelling remains available.".into(),
                ]);
            }
            Page::Findings => {
                match self.check_document(self.views.active) {
                    Ok((_, doc)) => rows.push(format!("Source: {doc} · editor-body UTF-8 bytes; logical scalar columns")),
                    Err(e) => rows.push(e.to_string()),
                }
                let current = self.current_check_report();
                let findings: Vec<_> = current.into_iter().flat_map(|r| &r.findings).filter(|d| !self.checks.once.contains(&d.id)).collect();
                if let Some(report) = current {
                    match &report.status {
                        ReportStatus::Complete => rows.push(format!("{} current suggestions · {} ms{} · {} unmappable omitted", findings.len(), report.elapsed_ms, if report.truncated { " · first 256 only" } else { "" }, report.omitted_invalid)),
                        ReportStatus::ScopeExcluded => rows.push("NOT CHECKED: notes/research scope excluded; s changes explicit policy.".into()),
                        ReportStatus::TooLarge => rows.push("NOT CHECKED: source exceeds 65,536-byte Harper cap. Existing spelling is unaffected.".into()),
                        ReportStatus::EngineFailed(e) => rows.push(format!("NOT CHECKED: {e}; r explicitly retries.")),
                    }
                } else {
                    let pending = self.checks.slots.iter().any(|s| s.pending);
                    rows.push(if pending { "Local check running; old results hidden. You can keep writing." } else { "No current result. r checks once; l enables optional live analysis." }.into());
                }
                let selection = self.checks.selected.min(findings.len().saturating_sub(1));
                for (i, finding) in findings.iter().enumerate() {
                    if i == selection { selected = Some(rows.len()); }
                    let point = current.and_then(|r| r.snapshot.point(finding.span.start)).unwrap_or((0, 0));
                    rows.push(format!("{} {}:{} · {} · {:?}/{:?} · {}", if i == selection { "›" } else { " " }, point.0 + 1, point.1 + 1, finding.rule.rule, finding.category, finding.context, clean(&finding.expected, 50)));
                    if i == selection {
                        rows.push(format!("Review: {}", clean(&finding.message, 500)));
                        if let Some(replacement) = finding.replacements.get(self.checks.alternative % finding.replacements.len().max(1)) {
                            rows.push(format!("Alternative {}/{}: {}{}", self.checks.alternative + 1, finding.replacements.len(), if replacement.text.is_empty() { "(remove)".into() } else { clean(&replacement.text, 80) }, if replacement.source_safe { " · a explicitly applies; Ctrl-Z undoes" } else { " · MANUAL EDIT: source markup/escape" }));
                        } else { rows.push("No automatic edit offered; Enter locates original source.".into()); }
                    }
                }
            }
        }
        (rows, selected)
    }
}
fn clean(text: &str, limit: usize) -> String {
    let mut out: String = text
        .chars()
        .take(limit)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if text.chars().count() > limit {
        out.push('…');
    }
    out
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Checks { page } = app.overlay else {
        return;
    };
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Writing checks · optional local analysis ")
        .border_style(Style::default().fg(t.accent));
    let inside = block.inner(area);
    f.render_widget(block, area);
    if inside.height < 7 || inside.width < 12 {
        return;
    }
    let footer_height = 5.min(inside.height - 2);
    let body = Rect::new(
        inside.x,
        inside.y,
        inside.width,
        inside.height - footer_height,
    );
    let footer = Rect::new(inside.x, body.y + body.height, inside.width, footer_height);
    let (rows, selection) = app.check_rows(page);
    let mut lines = vec![];
    let mut selected_line = None;
    for (i, row) in rows.iter().enumerate() {
        if selection == Some(i) {
            selected_line = Some(lines.len());
        }
        for text in crate::ui::wrap_words(row, body.width.saturating_sub(2) as usize) {
            lines.push(Line::styled(
                format!(" {text}"),
                Style::default().fg(if selection == Some(i) {
                    t.accent
                } else {
                    t.text
                }),
            ));
        }
    }
    let max = lines.len().saturating_sub(body.height as usize);
    let offset = app
        .checks
        .scroll
        .unwrap_or_else(|| {
            selected_line
                .unwrap_or(0)
                .saturating_sub(body.height as usize / 2)
        })
        .min(max);
    app.checks.render_fit.set((offset, max));
    f.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(offset)
                .take(body.height as usize)
                .collect::<Vec<_>>(),
        ),
        body,
    );
    let hints = match page {
        Page::Findings => {
            "↑↓ choose · Tab alternative · Enter source · a apply explicitly\ni ignore this pass · n mute novel rule · G personal rule (confirm)\nr run once · l live on/off · s settings · c pause/clear · u reload · Esc"
        }
        Page::Settings => {
            "p profile · d dialect · f footnotes · n notes · r research · v dialogue\nx/X clear novel/personal mutes · k revoke model permissions · u reload · Esc results"
        }
        _ => "y explicitly confirms · n/Esc cancels · F1 help",
    };
    f.render_widget(
        Paragraph::new(format!(
            "{}\n{hints}",
            clean(&app.checks.notice, inside.width as usize * 2)
        ))
        .style(Style::default().fg(t.dim))
        .wrap(Wrap { trim: false }),
        footer,
    );
}
