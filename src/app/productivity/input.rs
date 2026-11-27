use super::state::{GOALS, Input, Page, SPRINTS};
use crate::app::{App, Key, Overlay, spark};
use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration as CivilDuration, Months};
use grimoire_core::{
    accounting::Mode as WorkMode,
    challenges::{Category, Evidence},
    productivity::{Schedule, SprintKind},
};
use std::collections::BTreeSet;

impl App {
    pub fn open_productivity(&mut self, page: Page) {
        self.flush();
        self.productivity.activity.pause();
        self.checkpoint_productivity(true);
        if page == Page::Challenges {
            self.productivity.date = self.productivity.clock.now().day();
            if let Err(e) = self.productivity.load_challenge_day(&self.project.root) {
                self.msg = format!("Challenges unavailable: {e:#}");
            }
        }
        if page == Page::Calendar
            && !matches!(
                self.overlay,
                Overlay::ProductivityInput {
                    back: Page::Calendar,
                    ..
                }
            )
        {
            self.productivity.calendar_date = self.productivity.clock.now().wall.date_naive();
        }
        if page == Page::ChallengeFilters {
            self.productivity.draft_profile =
                Some(self.productivity.config.challenge_profile.clone());
        }
        self.overlay = Overlay::Productivity {
            page,
            sel: 0,
            scroll: None,
        };
        self.productivity.build_rows(page, 0);
        if page == Page::Achievements && !self.productivity.config.awards.is_empty() {
            self.observe_deck(grimoire_core::tutorial::Event::AchievementReviewed);
        }
    }
    fn productivity_number(&mut self, kind: Input, back: Page, value: String) {
        self.overlay = Overlay::ProductivityInput {
            kind,
            buf: value,
            back,
        };
    }
    pub fn on_productivity_key(&mut self, key: Key) {
        if matches!(
            self.overlay,
            Overlay::Productivity {
                page: Page::Challenges,
                ..
            }
        ) && key == Key::Char('S')
        {
            self.open_spark();
            self.refresh_spark_assignment();
            self.spark_page(spark::Page::Assignment, 0);
            return;
        }
        if matches!(self.overlay, Overlay::ProductivityInput { .. }) {
            self.on_productivity_input(key);
            return;
        }
        let Overlay::Productivity {
            page,
            mut sel,
            mut scroll,
        } = self.overlay
        else {
            return;
        };
        if key == Key::Esc || key == Key::Char('q') {
            if matches!(
                page,
                Page::GoalTypes | Page::SprintTypes | Page::ChallengeFilters
            ) {
                self.open_productivity(match page {
                    Page::GoalTypes => Page::Goals,
                    Page::SprintTypes => Page::Sprints,
                    _ => Page::Challenges,
                });
            } else {
                self.overlay = Overlay::None;
            }
            return;
        }
        if key == Key::Char('r') {
            self.reload_productivity();
            if page == Page::Challenges
                && let Err(e) = self.productivity.load_challenge_day(&self.project.root)
            {
                self.msg = format!("Challenge reload refused: {e:#}");
            }
            self.productivity.build_rows(page, sel);
            return;
        }
        if page == Page::Calendar {
            let date = self.productivity.calendar_date;
            let next = match key {
                Key::Left => date.checked_add_signed(CivilDuration::days(-1)),
                Key::Right => date.checked_add_signed(CivilDuration::days(1)),
                Key::Up => date.checked_add_signed(CivilDuration::days(-7)),
                Key::Down => date.checked_add_signed(CivilDuration::days(7)),
                Key::Char('[') => date.checked_sub_months(Months::new(1)),
                Key::Char(']') => date.checked_add_months(Months::new(1)),
                Key::Home => Some(self.productivity.clock.now().wall.date_naive()),
                _ => None,
            };
            if let Some(next) = next.filter(|day| (1..=9999).contains(&day.year())) {
                self.productivity.calendar_date = next;
                self.overlay = Overlay::Productivity {
                    page,
                    sel,
                    scroll: None,
                };
                self.productivity.build_rows(page, sel);
                return;
            }
        }
        let count = self.productivity.row_count(page);
        // differs: past either end of its list, ↑↓ go on to scroll the page,
        // and its pages type numbers and letters, so j/k stay letters here.
        if matches!(key, Key::Up | Key::Down | Key::Home | Key::End) && count > 0 {
            sel = match key {
                Key::Up => sel.saturating_sub(1),
                Key::Down => (sel + 1).min(count - 1),
                Key::Home => 0,
                _ => count - 1,
            };
            scroll = None;
            self.overlay = Overlay::Productivity { page, sel, scroll };
            self.productivity.build_rows(page, sel);
            return;
        }
        if matches!(key, Key::PageUp | Key::PageDown)
            || page != Page::Calendar && count == 0 && matches!(key, Key::Up | Key::Down)
        {
            let (offset, max) = self.productivity.render_fit.get();
            let step = if matches!(key, Key::Up | Key::Down) {
                1
            } else {
                10
            };
            scroll = Some(if matches!(key, Key::Down | Key::PageDown) {
                (offset + step).min(max)
            } else {
                offset.saturating_sub(step)
            });
            self.overlay = Overlay::Productivity { page, sel, scroll };
            return;
        }
        let result = (|| -> Result<()> {
            match page {
                Page::Overview => {
                    match key {
                        Key::Char('t' | 'm' | 'f' | 'a' | 'n' | 'd') => {
                            self.flush();
                            if !self.checkpoint_productivity(true) {
                                bail!(
                                    "pending measurements/sprint could not be checkpointed; settings unchanged; r explicitly reloads/retries"
                                );
                            }
                            self.productivity.update(|s| {
                                match key {
                                    Key::Char('t') => {
                                        s.tracking = !s.tracking;
                                        if !s.tracking
                                            && let Some(sprint) = &mut s.sprint
                                            && sprint.kind == SprintKind::NoEditing
                                        {
                                            sprint.metrics_incomplete = true;
                                        }
                                    }
                                    Key::Char('m') => s.mode = s.mode.next(),
                                    Key::Char('f') => s.include_footnotes = !s.include_footnotes,
                                    Key::Char('a') => s.achievements = !s.achievements,
                                    Key::Char('n') => s.notifications = !s.notifications,
                                    _ => s.challenges = !s.challenges,
                                }
                                Ok(())
                            })?;
                            self.productivity.activity.pause();
                            self.capture_productivity();
                            self.msg="Productivity preference recorded locally; original Markdown untouched".into();
                        }
                        Key::Char('i') => {
                            self.productivity_number(
                                Input::Idle,
                                page,
                                self.productivity.config.idle_seconds.to_string(),
                            );
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                Page::Goals => match key {
                    Key::Char('n') => {
                        self.open_productivity(Page::GoalTypes);
                        return Ok(());
                    }
                    Key::Char(' ') => {
                        self.productivity.update(|s| {
                            let g = s.goals.get_mut(sel).context("choose a goal")?;
                            g.enabled = !g.enabled;
                            Ok(())
                        })?;
                    }
                    Key::Delete => {
                        self.productivity.update(|s| {
                            if sel >= s.goals.len() {
                                bail!("choose a goal");
                            }
                            s.goals.remove(sel);
                            Ok(())
                        })?;
                        sel = sel.min(self.productivity.config.goals.len().saturating_sub(1));
                    }
                    _ => {}
                },
                Page::GoalTypes => {
                    if key == Key::Enter || key == Key::Char(' ') {
                        self.productivity_number(
                            Input::Goal(sel),
                            Page::Goals,
                            GOALS[sel.min(GOALS.len() - 1)].2.to_string(),
                        );
                        return Ok(());
                    }
                }
                Page::Calendar => {
                    match key {
                        Key::Char('n') => {
                            let year = self.productivity.clock.now().wall.year();
                            let schedule = Schedule::november(year)?;
                            let start = grimoire_core::journal::date(&schedule.start)?;
                            self.productivity.update(|s| {
                                if let Some(old)=s.schedule.take(){if s.schedule_history.len()>=120{bail!("120 earlier calendars retained; archive explicitly before replacing");}s.schedule_history.push(old);}
                                s.schedule = Some(schedule);
                                Ok(())
                            })?;
                            self.productivity.calendar_date = start;
                            scroll = None;
                            self.msg = format!(
                                "November {year} 50k schedule recorded; choose optional rest days. No past words inferred."
                            );
                        }
                        Key::Enter | Key::Char(' ') => {
                            let day = self.productivity.calendar_date.to_string();
                            self.productivity.update(|s| {
                                s.schedule
                                    .as_mut()
                                    .context("create a schedule first")?
                                    .toggle_rest(&day)
                            })?;
                            self.observe_deck(grimoire_core::tutorial::Event::CalendarConfigured);
                            self.msg="Explicit rest-day choice recorded; remaining-day pace recalculated".into();
                        }
                        Key::Char('b') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .start
                                .clone();
                            self.productivity_number(Input::ScheduleStart, page, value);
                            return Ok(());
                        }
                        Key::Char('e') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .end
                                .clone();
                            self.productivity_number(Input::ScheduleEnd, page, value);
                            return Ok(());
                        }
                        Key::Char('g') => {
                            let value = self
                                .productivity
                                .config
                                .schedule
                                .as_ref()
                                .context("create a schedule first")?
                                .target
                                .to_string();
                            self.productivity_number(Input::ScheduleTarget, page, value);
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                Page::Sprints => match key {
                    Key::Char('n') => {
                        self.open_productivity(Page::SprintTypes);
                        return Ok(());
                    }
                    Key::Char('b') => {
                        self.productivity_number(
                            Input::Sprint(SprintKind::Break),
                            page,
                            "5".into(),
                        );
                        return Ok(());
                    }
                    Key::Char('p' | 'x' | 'k') => {
                        self.flush();
                        if !self.checkpoint_productivity(true) {
                            bail!("checkpoint pending records before changing sprint state");
                        }
                        let now = self.productivity.clock.now();
                        let sprint = self
                            .productivity
                            .sprint
                            .as_ref()
                            .context("start a sprint first")?;
                        let reading = self.productivity.snapshot.lifetime(
                            sprint.metric(),
                            sprint.mode,
                            sprint.include_footnotes,
                        );
                        if key == Key::Char('p')
                            && self
                                .productivity
                                .sprint
                                .as_ref()
                                .is_some_and(|s| s.kind == SprintKind::NoEditing)
                            && !self.productivity.tracking()
                        {
                            bail!(
                                "enable operation tracking before resuming a no-editing experiment"
                            );
                        }
                        let sprint = self
                            .productivity
                            .sprint
                            .as_mut()
                            .context("Current sprint unavailable")?;
                        if key == Key::Char('x') {
                            sprint.stop(&now, reading);
                        } else if key == Key::Char('k') {
                            sprint.skip_break(&now)?;
                        } else if sprint.effective_paused() {
                            sprint.resume(&now, reading)?;
                        } else {
                            sprint.pause(&now, reading);
                        }
                        self.productivity.activity.pause();
                        self.productivity.update(|_| Ok(()))?;
                        self.msg =
                            "Sprint state recorded; no text deleted or edits suppressed".into();
                    }
                    _ => {}
                },
                Page::SprintTypes => {
                    if key == Key::Enter || key == Key::Char(' ') {
                        let (kind, _, value) = SPRINTS[sel.min(SPRINTS.len() - 1)];
                        self.productivity_number(
                            Input::Sprint(kind),
                            Page::Sprints,
                            value.to_string(),
                        );
                        return Ok(());
                    }
                }
                Page::Achievements => match key {
                    Key::Char('a') => {
                        self.productivity.update(|s| {
                            s.achievements = !s.achievements;
                            Ok(())
                        })?;
                    }
                    Key::Char('n') => {
                        self.productivity.update(|s| {
                            s.notifications = !s.notifications;
                            Ok(())
                        })?;
                    }
                    _ => {}
                },
                Page::Challenges => {
                    if matches!(key, Key::Left | Key::Right) {
                        let date = grimoire_core::journal::date(&self.productivity.date)?;
                        let next = date
                            .checked_add_signed(CivilDuration::days(if key == Key::Left {
                                -1
                            } else {
                                1
                            }))
                            .context("date outside supported calendar")?
                            .to_string();
                        grimoire_core::journal::date(&next)?;
                        self.productivity.date = next;
                        if let Err(e) = self.productivity.load_challenge_day(&self.project.root) {
                            self.msg = format!("Assignment unavailable: {e:#}");
                        }
                    } else if key == Key::Char('f') {
                        self.open_productivity(Page::ChallengeFilters);
                        return Ok(());
                    } else if matches!(key, Key::Char('a' | 'c' | 'x' | 'R')) {
                        if !self.productivity.config.challenges {
                            bail!("challenges disabled; no state changed");
                        }
                        self.flush();
                        if !self.checkpoint_productivity(true) && key != Key::Char('x') {
                            bail!(
                                "recorded metrics checkpoint unavailable; no measured acceptance/completion invented"
                            );
                        }
                        let now = self.productivity.clock.now();
                        let foot = self.productivity.config.include_footnotes;
                        let recent = if key == Key::Char('R') {
                            self.productivity
                                .recent(&self.project.root, &self.productivity.date)?
                        } else {
                            BTreeSet::new()
                        };
                        let profile = self.productivity.config.challenge_profile.clone();
                        let snapshot = &self.productivity.snapshot;
                        let store = self
                            .productivity
                            .day
                            .as_mut()
                            .context("no readable assignment; originals preserved")?;
                        let mut completed = None;
                        match key {
                            Key::Char('a') => {
                                store.update(|d| d.accept(sel, &now, snapshot, foot))?;
                                self.msg="Accepted; only subsequent recorded work in the exercise's mode counts".into();
                            }
                            Key::Char('R') => {
                                store.update(|d| {
                                    if d.assignments.is_empty() {
                                        d.new_selection(&now, &profile, &recent)
                                    } else {
                                        d.reroll_with_profile(sel, &now, &profile, &recent)
                                    }
                                })?;
                                self.msg="Explicit reroll recorded; original reference retained in history".into();
                            }
                            _ => {
                                let evidence = store.update(|d| {
                                    d.complete(sel, &now, snapshot, key == Key::Char('x'))
                                })?;
                                let a = &store.value.assignments[sel];
                                completed =
                                    Some((a.record_id.clone(), store.value.date.clone(), evidence));
                            }
                        }
                        if let Some((id, date, evidence)) = completed {
                            let award_id = format!("challenge-{date}-{id}");
                            self.productivity.update(|s| {
                                s.award(
                                    &award_id,
                                    if evidence == Evidence::Measured {
                                        "Completed a measured challenge"
                                    } else {
                                        "Self-reported a creative/reading exercise"
                                    },
                                    evidence == Evidence::Measured,
                                    &now.stamp(),
                                )?;
                                Ok(())
                            })?;
                            self.msg = format!(
                                "Challenge completion recorded: {evidence:?}; no quality/reading proof inferred"
                            );
                            if evidence == Evidence::Measured {
                                self.observe_deck(
                                    grimoire_core::tutorial::Event::DailyChallengeCompleted,
                                );
                            }
                        }
                    }
                }
                Page::ChallengeFilters => {
                    let profile = self
                        .productivity
                        .draft_profile
                        .as_mut()
                        .context("open filters again")?;
                    if key == Key::Enter {
                        let next = profile.clone();
                        self.productivity.update(|s| {
                            s.challenge_profile = next;
                            Ok(())
                        })?;
                        self.open_productivity(Page::Challenges);
                        self.msg =
                            "Future challenge filters saved; today's existing assignment retained"
                                .into();
                        return Ok(());
                    }
                    match key {
                        Key::Char(' ') if sel < 10 => {
                            let c = Category::ALL[sel];
                            if !profile.categories.remove(&c) {
                                profile.categories.insert(c);
                            }
                        }
                        Key::Left | Key::Right if sel == 10 => {
                            profile.mode = match (profile.mode, key) {
                                (None, Key::Right) | (Some(WorkMode::Revision), Key::Left) => {
                                    Some(WorkMode::Drafting)
                                }
                                (Some(WorkMode::Drafting), Key::Right)
                                | (Some(WorkMode::ReadThrough), Key::Left) => {
                                    Some(WorkMode::Revision)
                                }
                                (Some(WorkMode::Revision), Key::Right) | (None, Key::Left) => {
                                    Some(WorkMode::ReadThrough)
                                }
                                _ => None,
                            };
                        }
                        Key::Left | Key::Right if sel == 11 => {
                            profile.difficulty = if key == Key::Left {
                                profile.difficulty.saturating_sub(1).max(1)
                            } else {
                                (profile.difficulty + 1).min(3)
                            };
                        }
                        Key::Left | Key::Right if sel == 12 => {
                            let options = [5, 15, 30, 60, 120];
                            let at = options
                                .iter()
                                .position(|n| *n == profile.max_minutes)
                                .unwrap_or(2);
                            profile.max_minutes = options[if key == Key::Left {
                                at.saturating_sub(1)
                            } else {
                                (at + 1).min(4)
                            }];
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.msg = format!("Productivity action not recorded: {e:#}");
        }
        if matches!(self.overlay,Overlay::Productivity{page:p,..}if p==page) {
            sel = sel.min(self.productivity.row_count(page).saturating_sub(1));
            self.overlay = Overlay::Productivity { page, sel, scroll };
            self.refresh_productivity_snapshot();
            self.productivity.build_rows(page, sel);
        }
    }
    fn on_productivity_input(&mut self, key: Key) {
        let Overlay::ProductivityInput { kind, buf, back } = &mut self.overlay else {
            return;
        };
        match key {
            Key::Esc => {
                let back = *back;
                self.open_productivity(back);
            }
            Key::Backspace => {
                buf.pop();
            }
            Key::Char(c) if (c.is_ascii_digit() || c == '-') && buf.len() < 16 => buf.push(c),
            Key::Enter => {
                let (kind, text, back) = (*kind, buf.clone(), *back);
                self.flush();
                let result = (|| -> Result<()> {
                    match kind {
                        Input::ScheduleStart | Input::ScheduleEnd => {
                            grimoire_core::journal::date(&text)?;
                            self.productivity.update(|s| {
                                let schedule =
                                    s.schedule.as_mut().context("create schedule first")?;
                                if kind == Input::ScheduleStart {
                                    schedule.start = text;
                                } else {
                                    schedule.end = text;
                                }
                                Ok(())
                            })?;
                        }
                        Input::Goal(i) => {
                            self.new_productivity_goal(
                                i,
                                text.parse().context("enter a positive integer target")?,
                            )?;
                        }
                        Input::ScheduleTarget => {
                            let n = text.parse().context("enter a positive word target")?;
                            self.productivity.update(|s| {
                                s.schedule.as_mut().context("create schedule first")?.target = n;
                                Ok(())
                            })?;
                        }
                        Input::Idle => {
                            let n = text.parse().context("enter 1–900 seconds")?;
                            if !self.checkpoint_productivity(true) {
                                bail!("checkpoint pending operations first");
                            }
                            self.productivity.update(|s| {
                                s.idle_seconds = n;
                                Ok(())
                            })?;
                            self.productivity.activity.pause();
                        }
                        Input::Sprint(kind) => {
                            if !self.checkpoint_productivity(true) {
                                bail!("recorded metrics checkpoint unavailable");
                            }
                            let n: u64 = text.parse().context("enter a positive sprint target")?;
                            if n == 0
                                || n > 1_000_000
                                || matches!(
                                    kind,
                                    SprintKind::Timed | SprintKind::NoEditing | SprintKind::Break
                                ) && n > 240
                            {
                                bail!(
                                    "sprint target out of range (time 1–240 minutes; count 1–1,000,000)"
                                );
                            }
                            self.new_advanced_sprint(kind, n)?;
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        self.open_productivity(back);
                        self.msg =
                            "Local productivity setting recorded; manuscript originals untouched"
                                .into();
                    }
                    Err(e) => {
                        self.msg = format!("Not recorded: {e:#}; input and originals retained")
                    }
                }
            }
            _ => {}
        }
    }
}
