use super::state::GOALS;
use crate::app::App;
use anyhow::{Context, Result, bail};
use grimoire_core::{
    accounting::{Mode as WorkMode, Role},
    challenges::{self, Status},
    productivity::{Goal, Metric, Period, Sprint, SprintKind, SprintStatus},
};

impl App {
    pub(super) fn new_productivity_goal(&mut self, index: usize, target: u64) -> Result<()> {
        let (label, metric, _) = *GOALS.get(index).context("choose goal metric")?;
        let now = self.productivity.clock.now();
        let source = if metric == Metric::SceneWords {
            Some(
                self.open
                    .and_then(|i| self.metric_source(i))
                    .filter(|s| s.role == Role::Manuscript)
                    .context("open a manuscript scene before creating its goal")?
                    .path,
            )
        } else {
            None
        };
        let period = match index {
            0 | 5 | 6 | 7 => Period::Daily,
            1 => Period::Weekly,
            2 => Period::Monthly,
            8 => {
                let s = self
                    .productivity
                    .config
                    .schedule
                    .as_ref()
                    .context("configure a Calendar schedule first")?;
                Period::Deadline {
                    start: s.start.clone(),
                    end: s.end.clone(),
                }
            }
            _ => Period::SinceCreated,
        };
        self.productivity.update(|s| {
            let id = s.allocate()?;
            s.goals.push(Goal {
                id,
                label: label.into(),
                metric,
                target,
                period,
                created: now.day(),
                mode: if metric == Metric::RevisionOperations {
                    Some(WorkMode::Revision)
                } else {
                    None
                },
                source,
                enabled: true,
            });
            Ok(())
        })?;
        Ok(())
    }
    pub(super) fn new_advanced_sprint(&mut self, kind: SprintKind, target: u64) -> Result<()> {
        self.new_advanced_sprint_with_spark(kind, target, None)
    }
    pub(in crate::app) fn new_advanced_sprint_with_spark(
        &mut self,
        kind: SprintKind,
        target: u64,
        spark: Option<grimoire_core::spark::ResultRef>,
    ) -> Result<()> {
        if kind == SprintKind::Break
            && let Some(s) = &self.productivity.sprint
            && !matches!(s.status, SprintStatus::Completed | SprintStatus::Stopped)
        {
            let now = self.productivity.clock.now();
            let reading =
                self.productivity
                    .snapshot
                    .lifetime(s.metric(), s.mode, s.include_footnotes);
            self.productivity.update(|settings| {
                let id = settings.allocate()?;
                settings
                    .sprint
                    .as_mut()
                    .context("Current sprint unavailable")?
                    .start_break(id, &now, reading, target)
            })?;
            return Ok(());
        }
        if let Some(s) = &self.productivity.sprint
            && !matches!(s.status, SprintStatus::Completed | SprintStatus::Stopped)
        {
            bail!("Pause/resume or stop the current sprint before starting another");
        }
        if kind == SprintKind::NoEditing && !self.productivity.tracking() {
            bail!(
                "No-editing experiment requires operation tracking; no restriction is claimed while unobserved"
            );
        }
        let now = self.productivity.clock.now();
        let mut criterion = None;
        let mut mode = if kind == SprintKind::Revision {
            Some(WorkMode::Revision)
        } else {
            None
        };
        let mut link = None;
        let mut chosen_target = target;
        let mut footnotes = self.productivity.config.include_footnotes;
        if kind == SprintKind::Challenge {
            if self.productivity.day.is_none() {
                self.productivity.date = now.day();
                self.productivity.load_challenge_day(&self.project.root)?;
            }
            let day = &self
                .productivity
                .day
                .as_ref()
                .context("open/accept a daily challenge first")?
                .value;
            let (a, e, c) = day.assignments.iter()
                .filter(|a| a.status == Status::Accepted)
                .find_map(|a| {
                    let exercise = challenges::exercise(&a.record_id)?;
                    Some((a, exercise, exercise.measured.as_ref()?))
                })
                .context("accept a measured daily challenge first; subjective tasks use explicit self-report, not an invented counter")?;
            criterion = Some(c.clone());
            mode = Some(e.mode);
            footnotes = a.include_footnotes;
            let current = self
                .productivity
                .snapshot
                .lifetime(c.metric, Some(e.mode), footnotes)
                .context("measured challenge metric unavailable")?;
            let delta = current
                - a.baseline
                    .context("measured challenge baseline unavailable")?;
            if delta >= c.target as i128 {
                bail!("challenge criterion already reached; check/record its completion instead");
            }
            chosen_target = (c.target as i128 - delta.max(0)) as u64;
            link = Some((day.date.clone(), a.record_id.clone()));
        }
        let metric = criterion.as_ref().map_or(
            if kind == SprintKind::Revision {
                Metric::RevisionOperations
            } else {
                Metric::NetInput
            },
            |c| c.metric,
        );
        let reading = self.productivity.snapshot.lifetime(metric, mode, footnotes);
        self.productivity.update(|s| {
            if let Some(old) = s.sprint.take() {
                if s.sprint_history.len() >= 128 {
                    bail!("128 sprint results retained; archive explicitly before adding another");
                }
                s.sprint_history.push(old);
            }
            let id = s.allocate()?;
            let mut sprint = Sprint::new(id, kind, chosen_target, &now, reading, footnotes)?;
            sprint.mode = mode;
            sprint.criterion = criterion;
            sprint.challenge = link;
            sprint.spark = spark;
            s.sprint = Some(sprint);
            Ok(())
        })?;
        Ok(())
    }
}
