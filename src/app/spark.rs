//! Static cached Spark reader. Every generation/export is an explicit key;
//! paste/drag/hidden-editor routing is blocked in the common input path.
use super::*;
use anyhow::{Context, bail};
use grimoire_core::spark::{self as model, Engine, ExportKind, Kind as RecordKind, Mode, Reroll};
use ratatui::{
    Frame,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::{
    cell::Cell,
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Result,
    History,
    Favorites,
    Filters,
    Actions,
    Assignment,
    Provenance,
    ClearDismissals,
}
impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Result => "Story Spark · offline, non-AI",
            Self::History => "Spark history · retained references",
            Self::Favorites => "Spark favorites · retained references",
            Self::Filters => "Spark filters · future selections",
            Self::Actions => "Spark result · explicit actions",
            Self::Assignment => "Optional Spark challenge · self-report",
            Self::Provenance => "Spark provenance · original content",
            Self::ClearDismissals => "Clear Spark dismissals?",
        }
    }
    fn hints(self) -> &'static str {
        match self {
            Self::Result => {
                "1 plot  2 exercise  3 combined  4 daily · p/e/r reroll plot/exercise/both\nf favorite · x dismiss · s save · a actions · h history · F favorites · o filters"
            }
            Self::History | Self::Favorites => {
                "↑↓ choose · Enter show exact reference · Esc result\nHistory retains 128; favorites retain 64 independently; no manuscript changes"
            }
            Self::Filters => {
                "↑↓ row · ←→ category/level/minutes · Space genre · Enter save · Esc cancel\nNo selected genre means all; filters never silently relaxed; kept reroll component unchanged"
            }
            Self::Actions => {
                "↑↓ choose · Enter perform explicit action · Esc result\nNotes/task/outline are ordinary Markdown; new experiment is empty, excluded from compile"
            }
            Self::Assignment => {
                "a accept optional task · x self-report completion · Esc result\nNo quality or measured-writing completion is inferred from this report"
            }
            Self::Provenance => {
                "Source editions, original wording, hashes and license · PgUp/PgDn scroll\nMIT to extent rights subsist; AI-assisted drafting, not an independent human attestation"
            }
            Self::ClearDismissals => {
                "y clear only dismissal references · any other key cancels\nHistory, favorites, saved originals and daily assignments are retained"
            }
        }
    }
}
pub struct Spark {
    pub engine: Option<Engine<'static>>,
    pub warning: String,
    pub rows: Vec<String>,
    pub row_selection: Option<usize>,
    pub draft_filter: Option<model::Filter>,
    pub assignment: Option<model::Assignment>,
    pub assignment_date: String,
    pub day_warning: String,
    pub summary: String,
    pub render_fit: Cell<(usize, usize)>,
    pub animate_until: Option<Instant>,
}
impl Spark {
    pub fn load(root: &Path) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
            ^ (std::process::id() as u64).rotate_left(17);
        let (engine, warning) = match model::catalog()
            .and_then(|bank| Engine::load(root, seed, bank))
        {
            Ok(e) => (Some(e), String::new()),
            Err(e) => (
                None,
                format!("Spark unavailable: {e:#}; original state preserved; u explicitly reloads"),
            ),
        };
        let summary = engine.as_ref().map_or_else(
            || "Unavailable; originals preserved".into(),
            |e| {
                format!(
                    "{} original plots / {} exercises · static offline selection",
                    e.bank.plot_count, e.bank.exercise_count
                )
            },
        );
        Self {
            engine,
            warning,
            rows: vec![],
            row_selection: None,
            draft_filter: None,
            assignment: None,
            assignment_date: String::new(),
            day_warning: String::new(),
            summary,
            render_fit: Cell::new((0, 0)),
            animate_until: None,
        }
    }
    fn engine(&mut self) -> Result<&mut Engine<'static>> {
        self.engine
            .as_mut()
            .context("Spark state unavailable; original preserved, u reloads explicitly")
    }
    fn result(&self) -> Option<&model::ResultRef> {
        self.engine
            .as_ref()
            .and_then(|e| e.state().current.as_ref())
    }
    pub fn animating(&self) -> bool {
        self.animate_until.is_some_and(|end| Instant::now() < end)
    }
}
const ACTIONS: [&str; 13] = [
    "Save result as a Markdown note",
    "Favorite / unfavorite this exact result",
    "Dismiss this pairing (components may recur)",
    "Pin this result's saved source (same reference)",
    "Capture in the real scratchpad",
    "Capture as a Markdown outline note",
    "Capture as a Markdown task",
    "Create/open independent experiment (initially empty)",
    "Start a real suggested-duration timed sprint",
    "Assign as today's optional self-reported challenge",
    "Open today's optional challenge",
    "Review content provenance",
    "Clear dismissal references (confirm)",
];
impl App {
    pub fn open_spark(&mut self) {
        self.flush();
        self.tree_drag = None;
        self.spark.animate_until = None;
        self.refresh_spark_assignment();
        self.msg="Story Spark · 1 plot / 2 exercise / 3 combined / 4 persisted daily. Nothing is inserted automatically.".into();
        self.spark_page(Page::Result, 0);
    }
    pub(super) fn refresh_spark_assignment(&mut self) {
        let date = self.productivity.clock.now().day();
        self.spark.assignment_date = date.clone();
        match model::assignment(&self.project.root, &date) {
            Ok(s) => {
                self.spark.assignment = s.map(|s| s.value);
                self.spark.day_warning.clear();
            }
            Err(e) => {
                self.spark.assignment = None;
                self.spark.day_warning =
                    format!("Optional assignment unavailable: {e:#}; original preserved");
            }
        }
    }
    pub(super) fn spark_page(&mut self, page: Page, sel: usize) {
        self.overlay = Overlay::Spark {
            page,
            sel,
            scroll: None,
        };
        self.build_spark_rows(page, sel);
    }
    fn spark_list(&self, page: Page) -> Vec<model::ResultRef> {
        self.spark
            .engine
            .as_ref()
            .map(|e| match page {
                Page::History => e.state().history.iter().rev().cloned().collect(),
                Page::Favorites => e.state().favorites.values().rev().cloned().collect(),
                _ => vec![],
            })
            .unwrap_or_default()
    }
    fn spark_len(&self, page: Page) -> usize {
        match page {
            Page::History | Page::Favorites => self.spark_list(page).len(),
            Page::Filters => 4 + model::GENRES.len(),
            Page::Actions => ACTIONS.len(),
            _ => 0,
        }
    }
    pub fn build_spark_rows(&mut self, page: Page, sel: usize) {
        let mut rows = vec![];
        let mut selected = None;
        // Full notices are cached and scrollable, not merely truncated in the
        // footer. A failed action must be visible over the manuscript/status.
        if !self.msg.is_empty() {
            rows.push(format!("Status: {}", self.msg));
        }
        if !self.spark.warning.is_empty() {
            rows.push(self.spark.warning.clone());
        }
        match page {
            Page::Result=>{
                rows.push(self.spark.summary.clone());
                if let Some(engine)=&self.spark.engine {
                    if let Some(r)=&engine.state().current {
                        rows.push(format!("{} · {:?} · {} · {}",r.id,r.mode,r.date,r.zone.as_deref().unwrap_or("IANA zone unverified")));
                        if r.mode==Mode::Daily{rows.push("Fixed daily result; rerolls create separate exploration, never replace the day.".into());}
                        match engine.bank.resolve(r){
                            Ok((plot,exercise))=>for record in [plot,exercise].into_iter().flatten(){
                                rows.push(String::new());rows.push(format!("{} [{}] · {} · level {} · suggested {} min",if record.kind==RecordKind::Plot{"PLOT"}else{"EXERCISE"},record.id,record.category,record.difficulty,record.minutes));
                                rows.push(record.text.clone());
                                if !record.requires.is_empty(){rows.push(format!("Narrative conditions: {}",record.requires.iter().cloned().collect::<Vec<_>>().join(", ")));}
                                if let Some(mode)=record.work_mode{rows.push(format!("Work mode: {mode:?} · creative completion is self-reported, not automatically scored."));}
                            },
                            Err(e)=>rows.push(format!("{e:#}")),
                        }
                        if r.relaxed_recent{rows.push("Recent pool exhausted: recent repeats allowed explicitly in this receipt; filters/dismissals unchanged.".into());}
                        if engine.state().favorites.contains_key(&r.id){rows.push("★ Favorited; retained separately from history.".into());}
                        if engine.state().dismissed.contains(&r.key()){rows.push("Dismissed exact pairing. Components may be reused in a different pairing.".into());}
                    }else{rows.push("Choose 1, 2, 3 or 4. Opening this reader does not generate, assign, or change prose.".into());}
                    rows.push(String::new());rows.push(format!("{} history / 128 · {} favorites / 64 · {} dismissals / 128 · 12-result recent window",engine.state().history.len(),engine.state().favorites.len(),engine.state().dismissed.len()));
                    rows.push(format!("Roulette presentation: {} (l toggles); always static in focus/low-power.",if engine.state().roulette{"on"}else{"off"}));
                }
                if !self.spark.day_warning.is_empty(){rows.push(self.spark.day_warning.clone());}
                if let Some(a)=&self.spark.assignment{rows.push(format!("Optional {}: {:?} · {} (t opens; no measured completion inferred)",a.date,a.status,a.result.id));}
            },
            Page::History|Page::Favorites=>{
                rows.push(if page==Page::History{"128 most recent results; favorites and saved source receipts are independent."}else{"Up to 64 explicitly favorited results, including older corpus references."}.into());
                for (i,r) in self.spark_list(page).iter().enumerate(){if i==sel{selected=Some(rows.len());}let status=self.spark.engine.as_ref().map(|e|e.bank.resolve(r).is_ok()).unwrap_or(false);rows.push(format!("{} · {:?} · {} · plot {} / exercise {}{}",r.id,r.mode,r.date,r.plot.as_ref().map_or("—",|p|p.id.as_str()),r.exercise.as_ref().map_or("—",|p|p.id.as_str()),if status{""}else{" · historical content unavailable; ref retained"}));}
                if self.spark_len(page)==0{rows.push("No retained entries. Esc returns without changing anything.".into());}
            },
            Page::Filters=>{
                rows.push("Draft filters: empty category/genre selection means all. Enter explicitly saves; Esc cancels.".into());
                if let Some(f)=&self.spark.draft_filter {
                    let cats=|s:&BTreeSet<String>|if s.is_empty(){"all".into()}else{s.iter().cloned().collect::<Vec<_>>().join(", ")};
                    rows.push(format!("Plot category: {}",cats(&f.plot_categories)));rows.push(format!("Exercise category: {}",cats(&f.exercise_categories)));
                    rows.push(format!("Maximum starting difficulty: {} (editorial estimate, not quality)",f.max_difficulty));rows.push(format!("Maximum suggested minutes: {} (not measured completion)",f.max_minutes));
                    for genre in model::GENRES{rows.push(format!("[{}] genre {genre}",if f.genres.contains(genre){'x'}else{' '}));}
                    selected=Some(sel+1+usize::from(!self.spark.warning.is_empty())+usize::from(!self.msg.is_empty()));
                }
            },
            Page::Actions=>{
                rows.push("Actions use the selected result. They never replace the active scene; task/outline captures are Markdown notes, not M8 story facts.".into());
                for (i,label) in ACTIONS.iter().enumerate(){if i==sel{selected=Some(rows.len());}rows.push(label.to_string());}
            },
            Page::Assignment=>{
                rows.push(format!("Local civil date {} · optional, separately accepted, self-reported creative work",self.spark.assignment_date));
                if !self.spark.day_warning.is_empty(){rows.push(self.spark.day_warning.clone());}
                if let Some(a)=&self.spark.assignment{
                    rows.push(format!("{:?} · result {} · assigned {} / {}",a.status,a.result.id,a.assigned_at,a.zone));
                    if let Some(engine)=&self.spark.engine{match engine.bank.resolve(&a.result){Ok((p,e))=>for r in [p,e].into_iter().flatten(){rows.push(format!("{} · {}",r.id,r.text));},Err(e)=>rows.push(format!("{e:#}")),}}
                    rows.push(format!("Accepted: {} · Completed (self-reported): {}",a.accepted_at.as_deref().unwrap_or("not accepted"),a.completed_at.as_deref().unwrap_or("not reported")));
                }else{rows.push("No optional Spark assigned. a result action explicitly assigns one; opening this page does not.".into());}
            },
            Page::Provenance=>{
                rows.push("Original WritingDeck content: MIT to the extent rights subsist. AI-assisted drafting and agent editorial review; no exclusive copyright or independent human/legal attestation.".into());
                if let (Some(engine),Some(result))=(&self.spark.engine,self.spark.result()){
                    rows.push(format!("Corpus {} · seed {} · counter {}..{} · pool receipt {}",result.corpus_version,result.seed,result.counter_start,result.counter_end,result.pool_sha256));
                    rows.push("Replay uses exact component IDs/hashes; resampling additionally needs the selection state's recent/dismissed/filter context. Hashes do not authenticate imported history.".into());
                    match engine.bank.resolve(result){Ok((p,e))=>for r in [p,e].into_iter().flatten(){
                        rows.push(format!("{} · {} · {}",r.id,r.source.license,r.source.edition));rows.push(format!("Source {}:{} · section {}",r.source.file,r.source.line,r.source.section));rows.push(format!("Source-file SHA-256 {}",r.source.file_sha256));rows.push(format!("Text SHA-256 {}",r.sha256));rows.push(r.source.attribution.clone());rows.push(r.source.adaptation.clone());
                    },Err(e)=>rows.push(format!("{e:#}")),}
                }
                rows.push("No Plotto/OCR, modern cleanup, third-party prompt-list text or M5 exercise is counted in the 1,047-plot corpus. No model or network service runs when selecting results.".into());
            },
            Page::ClearDismissals=>rows.push("Clear the bounded dismissal list only? y confirms. This does not delete history, favorites, source files, dates, or optional assignments.".into()),
        }
        self.spark.rows = rows;
        self.spark.row_selection = selected;
    }
    fn after_spark_selection(&mut self) {
        self.spark.warning.clear();
        let animate = self
            .spark
            .engine
            .as_ref()
            .is_some_and(|e| e.state().roulette)
            && !self.focus_mode
            && !self.deck.state.low_power;
        self.spark.animate_until = animate.then(|| Instant::now() + Duration::from_millis(400));
        self.spark_page(Page::Result, 0);
        self.msg="Spark reference saved locally; no manuscript changed. f favorite, p/e/r reroll, a explicit result actions.".into();
    }
    fn generate_spark(&mut self, mode: Mode) -> Result<()> {
        let now = self.productivity.clock.now();
        self.spark.engine()?.generate(mode, &now)?;
        self.after_spark_selection();
        Ok(())
    }
    fn reroll_spark(&mut self, which: Reroll) -> Result<()> {
        let now = self.productivity.clock.now();
        self.spark.engine()?.reroll(which, &now)?;
        self.after_spark_selection();
        Ok(())
    }
    fn favorite_spark(&mut self) -> Result<()> {
        let added = self.spark.engine()?.favorite()?;
        if added
            && self
                .spark
                .result()
                .is_some_and(|r| r.mode == Mode::Exercise)
        {
            self.observe_deck(grimoire_core::tutorial::Event::SparkExerciseFavorited);
        }
        self.msg = if added {
            "Spark favorited; exact component references retained"
        } else {
            "Spark unfavorited; history/source originals retained"
        }
        .into();
        Ok(())
    }
    fn saved_spark_tutorial(&mut self, kind: ExportKind, newly_ready: bool) {
        if !newly_ready {
            return;
        }
        let Some(engine) = &self.spark.engine else {
            return;
        };
        let Some(r) = &engine.state().current else {
            return;
        };
        if kind == ExportKind::Save && r.mode == Mode::Plot {
            self.observe_deck(grimoire_core::tutorial::Event::SparkPlotSaved);
            return;
        }
        if kind != ExportKind::Scene || r.mode != Mode::Combined {
            return;
        }
        let mut at = r;
        let (mut plot, mut exercise, mut joint) = (false, false, false);
        let mut favorite = engine.state().favorites.contains_key(&r.id);
        for _ in 0..model::HISTORY_LIMIT {
            let Some(id) = &at.derived_from else { break };
            let Some(prior) = engine.state().history.iter().find(|h| &h.id == id) else {
                break;
            };
            if prior.mode != Mode::Combined && prior.mode != Mode::Daily {
                break;
            }
            let p = prior.plot != at.plot;
            let e = prior.exercise != at.exercise;
            plot |= p && !e;
            exercise |= e && !p;
            joint |= p && e;
            favorite |= engine.state().favorites.contains_key(&prior.id);
            at = prior;
        }
        if plot && exercise && joint && favorite {
            self.observe_deck(grimoire_core::tutorial::Event::SparkCombinedCaptured);
        }
    }
    pub fn capture_spark(&mut self, kind: ExportKind, pin: bool) -> Result<()> {
        self.workflow_prepare()?;
        let reference = self
            .spark
            .result()
            .cloned()
            .context("Select a Spark first")?;
        self.spark.engine()?.bank.resolve(&reference)?;
        let was_ready = self
            .spark
            .engine
            .as_ref()
            .unwrap()
            .state()
            .exports
            .iter()
            .any(|r| {
                r.result.id == reference.id && r.kind == kind && r.phase == model::Phase::Ready
            });
        let scratch_id = if kind == ExportKind::Scratch {
            let workflow = self
                .workflow
                .store
                .as_ref()
                .context("Scratchpad metadata unavailable; original preserved")?;
            workflow.state.validate()?;
            Some(
                self.spark
                    .engine
                    .as_ref()
                    .unwrap()
                    .state()
                    .exports
                    .iter()
                    .find(|r| r.result.id == reference.id && r.kind == ExportKind::Scratch)
                    .and_then(|r| r.scratch_id)
                    .unwrap_or(workflow.state.next_id),
            )
        } else {
            None
        };
        // A new scene is empty. Its prompt lives once in a separate saved note.
        if kind == ExportKind::Scene {
            self.spark.engine()?.export(ExportKind::Save, None)?;
        }
        let result = (|| -> Result<PathBuf> {
            let relative = self.spark.engine()?.export(kind, scratch_id)?;
            if let Some(id) = scratch_id {
                let expected = model::markdown(model::catalog()?, &reference, ExportKind::Scratch)?;
                self.workflow
                    .store
                    .as_mut()
                    .unwrap()
                    .adopt_created_note(id, grimoire_core::sync::Fingerprint::of(&expected))?;
                self.workflow.selected = Some(id);
                self.workflow.life = grimoire_core::pins::Life::Active;
            }
            Ok(relative)
        })();
        // A partially successful export remains visible/recoverable, never hidden
        // by an error or overwritten on retry. Existing sources were saved first.
        let reload = self.reload_tree();
        self.refresh_workflow_sources();
        let relative = result?;
        reload?;
        if pin {
            let existing = self
                .pins
                .state()
                .pins
                .iter()
                .find(|p| p.source == relative && p.life == grimoire_core::pins::Life::Active)
                .map(|p| p.id);
            if let Some(id) = existing {
                // Retrying this receipt-driven action selects the same card,
                // retaining that card's independently controlled scroll.
                self.pins.update(|s| {
                    s.selected = Some(id);
                    s.sidebar_visible = true;
                    Ok(())
                })?;
                self.refresh_pin_sources();
            } else {
                self.try_pin_document(&self.project.root.join(&relative), None)?;
            }
        }
        if let Some(id) = scratch_id {
            self.workflow.selected = Some(id);
            self.edit_scratchpad(true);
            if self.open.is_none_or(|index| {
                self.project.nodes[index].path != self.project.root.join(&relative)
            }) || self.overlay != Overlay::None
            {
                bail!(
                    "Scratch source captured safely but its original editor could not be opened: {}",
                    self.msg
                );
            }
        } else if kind == ExportKind::Scene {
            let index = self
                .project
                .nodes
                .iter()
                .position(|n| n.path == self.project.root.join(&relative))
                .context("New independent experimental source not loaded")?;
            if self.project.nodes[index].compile {
                bail!(
                    "Experimental source unexpectedly included in compile; inspect original before editing"
                );
            }
            self.open_scene(index);
            self.overlay = Overlay::None;
        } else {
            self.spark_page(Page::Result, 0);
        }
        self.saved_spark_tutorial(kind, !was_ready);
        self.msg = format!(
            "Spark {} · {} · original scene preserved{}",
            kind.label(),
            relative.display(),
            if pin {
                " · same saved-source reference pinned"
            } else {
                ""
            }
        );
        Ok(())
    }
    fn start_spark_sprint(&mut self) -> Result<()> {
        let reference = self
            .spark
            .result()
            .cloned()
            .context("Select a result first")?;
        let (_, exercise) = model::catalog()?.resolve(&reference)?;
        let minutes = exercise.map_or(20, |e| e.minutes);
        self.capture_productivity();
        self.refresh_productivity_snapshot();
        self.new_advanced_sprint_with_spark(
            grimoire_core::productivity::SprintKind::Timed,
            minutes as u64,
            Some(reference),
        )?;
        self.open_productivity(super::productivity::Page::Sprints);
        self.msg = format!(
            "Real {minutes}-minute timed sprint started with a Spark reference; timer completion does not prove creative completion"
        );
        Ok(())
    }
    fn spark_action(&mut self, index: usize) -> Result<()> {
        match index {
            0 => self.capture_spark(ExportKind::Save, false),
            1 => self.favorite_spark(),
            2 => {
                self.spark.engine()?.dismiss()?;
                self.msg =
                    "Exact Spark pairing dismissed; components can appear in other pairings".into();
                Ok(())
            }
            3 => self.capture_spark(ExportKind::Save, true),
            4 => self.capture_spark(ExportKind::Scratch, false),
            5 => self.capture_spark(ExportKind::Outline, false),
            6 => self.capture_spark(ExportKind::Task, false),
            7 => self.capture_spark(ExportKind::Scene, false),
            8 => self.start_spark_sprint(),
            9 => {
                let now = self.productivity.clock.now();
                let reference = self
                    .spark
                    .result()
                    .cloned()
                    .context("Select a result first")?;
                model::catalog()?.resolve(&reference)?;
                model::assign(&self.project.root, &reference, &now)?;
                self.refresh_spark_assignment();
                self.spark_page(Page::Assignment, 0);
                self.msg="Optional Spark assigned; a accepts, x explicitly self-reports only after acceptance".into();
                Ok(())
            }
            10 => {
                self.refresh_spark_assignment();
                self.spark_page(Page::Assignment, 0);
                Ok(())
            }
            11 => {
                self.spark_page(Page::Provenance, 0);
                Ok(())
            }
            12 => {
                self.spark_page(Page::ClearDismissals, 0);
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn adjust_spark_filter(&mut self, sel: usize, step: isize) -> Result<()> {
        let bank = model::catalog()?;
        let f = self
            .spark
            .draft_filter
            .as_mut()
            .context("No draft filters")?;
        match sel {
            0 | 1 => {
                let kind = if sel == 0 {
                    RecordKind::Plot
                } else {
                    RecordKind::Exercise
                };
                let choices = bank.categories(kind);
                let current = if sel == 0 {
                    &mut f.plot_categories
                } else {
                    &mut f.exercise_categories
                };
                let i = current
                    .iter()
                    .next()
                    .and_then(|c| choices.iter().position(|s| s == c))
                    .map_or(0, |i| i + 1);
                let next = (i as isize + step).rem_euclid((choices.len() + 1) as isize) as usize;
                current.clear();
                if next > 0 {
                    current.insert(choices[next - 1].clone());
                }
            }
            2 => f.max_difficulty = (f.max_difficulty as isize - 1 + step).rem_euclid(3) as u8 + 1,
            3 => {
                const MINUTES: [u16; 7] = [10, 15, 20, 30, 45, 60, 120];
                let i = MINUTES
                    .iter()
                    .position(|m| *m == f.max_minutes)
                    .unwrap_or(3);
                f.max_minutes =
                    MINUTES[(i as isize + step).rem_euclid(MINUTES.len() as isize) as usize];
            }
            n if n < 4 + model::GENRES.len() => {
                let genre = model::GENRES[n - 4];
                if !f.genres.remove(genre) {
                    f.genres.insert(genre.into());
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn on_spark_key(&mut self, key: Key) {
        let Overlay::Spark {
            page,
            mut sel,
            scroll,
        } = self.overlay
        else {
            return;
        };
        if page == Page::ClearDismissals {
            let result = if key == Key::Char('y') {
                self.spark.engine().and_then(|e| e.clear_dismissals())
            } else {
                Ok(())
            };
            self.spark_page(Page::Result, 0);
            if let Err(e) = result {
                self.msg = format!("Spark action not saved: {e:#}");
            }
            return;
        }
        let len = self.spark_len(page);
        if len > 0 && matches!(key, Key::Up | Key::Down | Key::Home | Key::End) {
            sel = match key {
                Key::Up => sel.saturating_sub(1),
                Key::Down => (sel + 1).min(len - 1),
                Key::Home => 0,
                Key::End => len - 1,
                _ => sel,
            };
            self.spark_page(page, sel);
            return;
        }
        if matches!(key, Key::PageUp | Key::PageDown)
            || len == 0 && matches!(key, Key::Up | Key::Down)
        {
            let (offset, max) = self.spark.render_fit.get();
            let delta = if matches!(key, Key::Up | Key::Down) {
                1
            } else {
                8
            };
            let next = if matches!(key, Key::PageUp | Key::Up) {
                offset.saturating_sub(delta)
            } else {
                offset.saturating_add(delta).min(max)
            };
            self.overlay = Overlay::Spark {
                page,
                sel,
                scroll: Some(next),
            };
            return;
        }
        if key == Key::Esc {
            self.spark.animate_until = None;
            if page == Page::Result {
                self.overlay = Overlay::None;
            } else {
                self.spark.draft_filter = None;
                self.spark_page(Page::Result, 0);
            }
            return;
        }
        let outcome = (|| -> Result<()> {
            if key == Key::F(1) {
                self.open_help(Some("story-spark"));
                return Ok(());
            }
            match page {
                Page::Filters => match key {
                    Key::Left => self.adjust_spark_filter(sel, -1)?,
                    Key::Right | Key::Char(' ') => self.adjust_spark_filter(sel, 1)?,
                    Key::Enter => {
                        let f = self
                            .spark
                            .draft_filter
                            .clone()
                            .context("No draft filters")?;
                        self.spark.engine()?.set_filter(f)?;
                        self.spark.draft_filter = None;
                        self.spark_page(Page::Result, 0);
                        self.msg="Spark filters saved for future/new components; existing daily and kept components unchanged".into();
                    }
                    _ => {}
                },
                Page::Actions if key == Key::Enter => self.spark_action(sel)?,
                Page::History | Page::Favorites if key == Key::Enter => {
                    let records = self.spark_list(page);
                    if let Some(r) = records.get(sel) {
                        self.spark.engine()?.show(&r.id)?;
                        self.spark_page(Page::Result, 0);
                    }
                }
                Page::Assignment if matches!(key, Key::Char('a' | 'x')) => {
                    let now = self.productivity.clock.now();
                    if now.day() != self.spark.assignment_date {
                        self.refresh_spark_assignment();
                        bail!(
                            "Local civil date changed; review the newly displayed optional assignment before acting"
                        );
                    }
                    model::accept_or_complete(
                        &self.project.root,
                        &now.day(),
                        &now,
                        key == Key::Char('x'),
                    )?;
                    self.refresh_spark_assignment();
                    self.msg = if key == Key::Char('x') {
                        "Optional Spark completed — SELF-REPORTED, not measured"
                    } else {
                        "Optional Spark accepted; no writing inferred"
                    }
                    .into();
                }
                Page::Result => match key {
                    Key::Char('1') => self.generate_spark(Mode::Plot)?,
                    Key::Char('2') => self.generate_spark(Mode::Exercise)?,
                    Key::Char('3') => self.generate_spark(Mode::Combined)?,
                    Key::Char('4') => self.generate_spark(Mode::Daily)?,
                    Key::Char('p') => self.reroll_spark(Reroll::Plot)?,
                    Key::Char('e') => self.reroll_spark(Reroll::Exercise)?,
                    Key::Char('r') => self.reroll_spark(Reroll::Both)?,
                    Key::Char('f') => self.favorite_spark()?,
                    Key::Char('x') => {
                        self.spark.engine()?.dismiss()?;
                        self.msg = "Exact pairing dismissed; components may recur elsewhere".into();
                    }
                    Key::Char('s') => self.capture_spark(ExportKind::Save, false)?,
                    Key::Char('a') => self.spark_page(Page::Actions, 0),
                    Key::Char('h') => self.spark_page(Page::History, 0),
                    Key::Char('F') => self.spark_page(Page::Favorites, 0),
                    Key::Char('o') => {
                        let f = self.spark.engine()?.state().filter.clone();
                        self.spark.draft_filter = Some(f);
                        self.spark_page(Page::Filters, 0);
                    }
                    Key::Char('t') => {
                        self.refresh_spark_assignment();
                        self.spark_page(Page::Assignment, 0);
                    }
                    Key::Char('v') => self.spark_page(Page::Provenance, 0),
                    Key::Char('u') => {
                        if self.spark.engine.as_ref().is_some_and(|e| e.store.exists())
                            && grimoire_core::statefile::read(
                                &self.project.root,
                                Path::new(".grimoire/writingdeck/spark/state.json"),
                            )?
                            .is_none()
                        {
                            bail!(
                                "Previously loaded Spark metadata is missing; cached references retained. Restore the original before reloading."
                            );
                        }
                        let loaded = Spark::load(&self.project.root);
                        if loaded.engine.is_none() {
                            self.spark.warning = loaded.warning;
                            bail!(
                                "Reload refused; preserved metadata and cached references retained"
                            );
                        }
                        self.spark = loaded;
                        self.refresh_spark_assignment();
                        self.spark_page(Page::Result, 0);
                        self.msg =
                            "Spark explicitly reloaded; corrupt/future originals were not replaced"
                                .into();
                    }
                    Key::Char('l') => {
                        self.spark.engine()?.store.update(|s| {
                            s.roulette = !s.roulette;
                            Ok(())
                        })?;
                        self.msg="Roulette is presentation only; focus/low-power always stay static; no seed or history changed".into();
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = outcome {
            self.msg = format!("Spark action not completed: {e:#}; originals retained");
        }
        if let Overlay::Spark {
            page,
            sel,
            scroll: now_scroll,
        } = self.overlay
        {
            self.build_spark_rows(page, sel);
            if now_scroll == scroll {
                self.overlay = Overlay::Spark { page, sel, scroll };
            }
        }
    }
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Spark { page, scroll, .. } = app.overlay else {
        return;
    };
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {} ", page.title()))
        .border_style(Style::default().fg(t.accent));
    let inside = block.inner(area);
    f.render_widget(block, area);
    if inside.height < 6 || inside.width < 10 {
        return;
    }
    let footer_h = 6.min(inside.height - 2);
    let body = Rect::new(inside.x, inside.y, inside.width, inside.height - footer_h);
    let footer = Rect::new(inside.x, body.y + body.height, inside.width, footer_h);
    let mut lines = vec![];
    let mut selected = None;
    for (i, row) in app.spark.rows.iter().enumerate() {
        let chosen = app.spark.row_selection == Some(i);
        if chosen {
            selected = Some(lines.len());
        }
        for text in crate::ui::wrap_words(row, body.width.saturating_sub(2) as usize) {
            lines.push(Line::styled(
                format!("{}{text}", if chosen { "› " } else { "  " }),
                Style::default().fg(if chosen { t.accent } else { t.text }),
            ));
        }
    }
    let max = lines.len().saturating_sub(body.height as usize);
    let offset = scroll
        .unwrap_or_else(|| {
            selected
                .unwrap_or(0)
                .saturating_sub(body.height as usize / 2)
        })
        .min(max);
    app.spark.render_fit.set((offset, max));
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
    let animation = if !app.focus_mode && !app.deck.state.low_power && app.spark.animating() {
        [" · ○··", " · ·○·", " · ··○"][(app.frame as usize) % 3]
    } else {
        ""
    };
    f.render_widget(
        Paragraph::new(format!(
            "{}\nPgUp/PgDn {}/{} · u reload · v source · t optional · Esc · F1 help{}",
            page.hints(),
            offset,
            max,
            animation
        ))
        .style(Style::default().fg(t.dim))
        .wrap(Wrap { trim: false }),
        Rect::new(
            footer.x,
            footer.y,
            footer.width,
            footer.height.saturating_sub(2),
        ),
    );
    // Also surface out-of-controller notices such as a refused normal quit.
    f.render_widget(
        Paragraph::new(app.msg.as_str())
            .style(Style::default().fg(t.accent))
            .wrap(Wrap { trim: false }),
        Rect::new(
            footer.x,
            footer.y + footer.height.saturating_sub(2),
            footer.width,
            2,
        ),
    );
}
