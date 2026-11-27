use super::state::{GOALS, Page, Productivity, SPRINTS};
use grimoire_core::{
    accounting::{Role, Totals},
    challenges::{self, Category},
    productivity::{Metric, SprintKind, SprintStatus},
};
use std::collections::{BTreeMap, BTreeSet};

impl Productivity {
    pub(super) fn build_rows(&mut self, page: Page, sel: usize) {
        let p = &*self;
        let mut rows = Vec::new();
        let mut selected = None;
        if !p.warning.is_empty() {
            rows.push(p.warning.clone());
        }
        if !p.ledger.warning.is_empty() {
            rows.push(p.ledger.warning.clone());
        }
        match page {
            Page::Overview => {
                let footnotes = p.config.include_footnotes;
                rows.push(format!(
                    "Tracking {} · {:?} · idle cutoff {}s · footnotes {}",
                    if p.tracking() {
                        "on"
                    } else {
                        "off/unavailable"
                    },
                    p.config.mode,
                    p.config.idle_seconds,
                    if footnotes { "included" } else { "separate" }
                ));
                rows.push("Measured replacements, not baselines/Git; words use source policy v1, not rendered/native export counts.".into());
                let now = p.clock.now(); // Called by input/checkpoint, never by draw.
                rows.push(format!(
                    "Local {} · {} · persisted {} groups / pending {}",
                    now.day(),
                    now.zone
                        .as_deref()
                        .unwrap_or("IANA zone unavailable; fixed offset recorded"),
                    p.ledger.known.len(),
                    p.ledger.pending_count()
                ));
                if !p.snapshot.complete {
                    rows.push("History incomplete: following values are known cached subsets, not complete totals; goals do not guess missing values.".into());
                }
                for (role, name) in [
                    (Role::Manuscript, "Manuscript"),
                    (Role::Notes, "Notes/characters/places"),
                    (Role::Research, "Research"),
                    (Role::Other, "Other"),
                ] {
                    let t = Totals::collect(p.ledger.known.iter(), &now.day(), &now.day(), role);
                    rows.push(format!(
                        "{name}: input +{} / −{} · net {} · {} word gaps",
                        t.input.added_words(footnotes),
                        t.input.removed_words(footnotes),
                        t.net(footnotes),
                        t.input.gaps + t.replay.gaps
                    ));
                    rows.push(format!(
                        "  keyboard +{} · paste +{} · undo/redo/restore net {} · footnotes +{} / −{}",
                        t.typed.added_words(footnotes),
                        t.pasted.added_words(footnotes),
                        t.replay.net(footnotes),
                        t.input.foot_added,
                        t.input.foot_removed
                    ));
                    rows.push(format!(
                        "  inter-edit active {:.1}s · keyboard-pair {:.1}s · unassigned {:.1}s",
                        t.active_ms as f64 / 1000.,
                        t.typed_pair_ms as f64 / 1000.,
                        t.unassigned_ms as f64 / 1000.
                    ));
                }
                rows.push("No startup, idle tail or offline time. Keyboard-pair intervals are NOT a claim of proven exclusive typing.".into());
                rows.push("Large-source word gaps remain unknown; raw inserted/deleted characters still record. No automatic manuscript rewriting.".into());
                rows.push(format!(
                    "Milestones {} · notices {} · challenges {} · {} unrecorded operations",
                    p.config.achievements,
                    p.config.notifications,
                    p.config.challenges,
                    p.unrecorded.operations
                ));
            }
            Page::Goals => {
                rows.push("Recorded-operation goals differ from native existing-source totals; no legacy baseline becomes gross input.".into());
                selected = Some(rows.len() + sel);
                for g in &p.config.goals {
                    let value =
                        p.snapshot
                            .goal(g, &p.clock.now().day(), p.config.include_footnotes);
                    rows.push(format!(
                        "{} {} · {} / {} · {} · {:?}",
                        if g.enabled { "[on]" } else { "[off]" },
                        g.label,
                        value.map_or("unavailable".into(), |n| n.to_string()),
                        g.target,
                        g.metric.label(),
                        g.period
                    ));
                }
                if p.config.goals.is_empty() {
                    rows.push(
                        "No advanced goals yet. n chooses a configurable \
                         daily/weekly/monthly/project/scene/time/revision/deadline target."
                            .into(),
                    );
                }
                rows.push("The footnote setting applies to recorded totals and word goals. Native goals use the manuscript's own word count.".into());
            }
            Page::GoalTypes => {
                selected = Some(rows.len() + sel);
                for (label, metric, target) in GOALS {
                    rows.push(format!("{label} · suggested {target} · {}", metric.label()));
                }
            }
            Page::Calendar => {
                if let Some(schedule) = &p.config.schedule {
                    let now = p.clock.now();
                    let net = p.snapshot.value(
                        Metric::NetInput,
                        &schedule.start,
                        &schedule.end,
                        None,
                        None,
                        p.config.include_footnotes,
                    );
                    rows.push(format!(
                        "{} → {} · target {} recorded net words · current {}",
                        schedule.start,
                        schedule.end,
                        schedule.target,
                        net.map_or("unavailable".into(), |n| n.to_string())
                    ));
                    let day = p.calendar_date;
                    let date = day.to_string();
                    if schedule.start <= date && date <= schedule.end {
                        let recorded = p
                            .ledger
                            .known
                            .iter()
                            .any(|e| e.source.role == Role::Manuscript && e.date == date);
                        let value = if recorded {
                            p.snapshot
                                .value(
                                    Metric::NetInput,
                                    &date,
                                    &date,
                                    None,
                                    None,
                                    p.config.include_footnotes,
                                )
                                .map_or("unavailable".into(), |n| format!("{n:+}"))
                        } else {
                            "not recorded".into()
                        };
                        let kind = if schedule.rest_days.contains(&date) {
                            "rest"
                        } else {
                            "work"
                        };
                        rows.push(format!("{date} {} {kind} · {value}", day.format("%a")));
                    }
                    if let Some(net) = net
                        && let Ok(pace) = schedule.pace(&now.day(), net)
                    {
                        rows.push(format!("Working days left {} · required/day {} · ahead/behind {} (end-of-day plan)",pace.working_days_left,pace.required_per_day.map_or("unavailable".into(),|n|n.to_string()),pace.ahead_behind.map_or("no workdays".into(),|n|format!("{n:+}"))));
                        let dates: BTreeSet<_> = p
                            .ledger
                            .known
                            .iter()
                            .filter(|e| e.source.role == Role::Manuscript)
                            .map(|e| e.date.clone())
                            .collect();
                        let history: BTreeMap<_, _> = dates
                            .into_iter()
                            .filter_map(|d| {
                                p.snapshot
                                    .value(
                                        Metric::NetInput,
                                        &d,
                                        &d,
                                        None,
                                        None,
                                        p.config.include_footnotes,
                                    )
                                    .map(|n| (d, n))
                            })
                            .collect();
                        rows.push(format!("Forecast {} · assumes mean of last 3–7 recorded work dates; missing days are not zero",schedule.forecast(&now.day(),net,&history).ok().flatten().unwrap_or_else(||"unavailable".into())));
                    }
                } else {
                    rows.push("No calendar schedule yet. n explicitly creates this year's offline November 50,000-word preset.".into());
                    rows.push("Start/end, target and optional rest days are configurable. No external account, endorsement or inferred past writing.".into());
                }
            }
            Page::Sprints => {
                rows.push("Elapsed sprint time is monotonic, not active-writing proof. Restart requires explicit resume; offline time is never added.".into());
                selected = None;
                if let Some(s) = &p.sprint {
                    rows.push(format!(
                        "Current #{} {:?} · {} · {:.1} min · progress {} / {}{}{}",
                        s.id,
                        s.kind,
                        if s.effective_paused() {
                            "paused (resume explicitly)"
                        } else {
                            match s.status {
                                SprintStatus::Running => "running",
                                SprintStatus::Completed => "completed",
                                SprintStatus::Stopped => "stopped",
                                SprintStatus::Paused => "paused",
                            }
                        },
                        s.elapsed_ms as f64 / 60_000.,
                        if s.kind == SprintKind::Timed {
                            format!("{:.1} min elapsed", s.elapsed_ms as f64 / 60_000.)
                        } else if s.metrics_incomplete {
                            "unavailable".into()
                        } else {
                            s.progress.to_string()
                        },
                        if s.kind == SprintKind::Timed {
                            format!("{} min timer", s.target)
                        } else {
                            s.target.to_string()
                        },
                        if s.deletion_observed {
                            " · deletion observed; text retained"
                        } else {
                            ""
                        },
                        if s.challenge.is_some() {
                            " · challenge linked"
                        } else {
                            ""
                        }
                    ));
                    if let Some(spark) = &s.spark {
                        rows.push(format!("Spark source {} · {} · exact component references; timed completion is not creative completion", spark.id, spark.corpus_version));
                    }
                    rows.push(format!(
                        "Policy: {} · mode {:?} · footnotes {} · checkpoint {}",
                        s.metric().label(),
                        s.mode,
                        s.include_footnotes,
                        s.last_checkpoint
                    ));
                }
                if let Some(s) = &p.sprint {
                    if let Some(b) = &s.break_timer {
                        rows.push(format!("Break #{} {} · {:.1} / {} min; work paused · p toggles break · k skips/records",b.id,if b.effective_paused(){"paused; explicit resume required"}else{"running"},b.elapsed_ms as f64/60000.,b.target));
                    }
                    if !s.breaks.is_empty() {
                        rows.push(format!(
                            "{} completed/stopped breaks retained; no forced work resume",
                            s.breaks.len()
                        ));
                    }
                }
                for s in p.config.sprint_history.iter().rev() {
                    rows.push(format!(
                        "#{} {:?} {:?} · {:.1} min · {} / {}{}",
                        s.id,
                        s.kind,
                        s.status,
                        s.elapsed_ms as f64 / 60_000.,
                        if s.kind == SprintKind::Timed {
                            format!("{:.1} min elapsed", s.elapsed_ms as f64 / 60_000.)
                        } else if s.metrics_incomplete {
                            "unavailable".into()
                        } else {
                            s.progress.to_string()
                        },
                        if s.kind == SprintKind::Timed {
                            format!("{} min timer", s.target)
                        } else {
                            s.target.to_string()
                        },
                        if s.deletion_observed {
                            " · deletions retained"
                        } else {
                            ""
                        }
                    ));
                }
                if p.sprint.is_none() && p.config.sprint_history.is_empty() {
                    rows.push("No advanced sprints yet; existing F3/Pomodoro and basic sprint controls remain separate.".into());
                }
                rows.push("p pauses/resumes only this sprint; edits while paused are excluded. x stops without deleting prose. b starts an optional timed break.".into());
            }
            Page::SprintTypes => {
                selected = Some(rows.len() + sel);
                for (_, label, target) in SPRINTS {
                    rows.push(format!("{label} · suggested target {target}"));
                }
            }
            Page::Achievements => {
                rows.push(format!(
                    "Milestones {} · notices {} · no punitive streak resets",
                    p.config.achievements, p.config.notifications
                ));
                selected = Some(rows.len() + sel);
                for a in &p.config.awards {
                    rows.push(format!(
                        "{} · {} · {}",
                        a.label,
                        if a.measured {
                            "recorded measurement"
                        } else {
                            "explicit action / self-report, not text-quality verification"
                        },
                        a.earned_at
                    ));
                }
                if p.config.awards.is_empty() {
                    rows.push("No milestones recorded yet. Actual keyboard word additions, Revision-mode edits, optional sprints, saved finished sources and learning can count.".into());
                }
                rows.push("Disabling retains your collection but awards nothing new. Nothing forces a notification into focus writing.".into());
            }
            Page::Challenges => {
                rows.push(format!(
                    "{} · creation zone {} · {} original exercises across 10 categories",
                    p.date,
                    p.day
                        .as_ref()
                        .and_then(|d| d.value.zone.as_deref())
                        .unwrap_or("unavailable"),
                    challenges::catalog().len()
                ));
                if let Some(day) = &p.day {
                    rows.push(format!(
                        "Saved profile: {:?}, difficulty ≤{}, ≤{} min · {} explicit rerolls",
                        day.value.profile.mode,
                        day.value.profile.difficulty,
                        day.value.profile.max_minutes,
                        day.value.rerolls.len()
                    ));
                    selected = Some(rows.len() + sel);
                    for a in &day.value.assignments {
                        if let Some(e) = challenges::exercise(&a.record_id) {
                            rows.push(format!(
                                "[{:?}] {} · {} · {} min · level {}{}",
                                a.status,
                                e.title,
                                e.category.label(),
                                e.minutes,
                                e.difficulty,
                                a.evidence.map_or("".into(), |k| format!(" · {k:?}"))
                            ));
                        }
                    }
                    if let Some(a) = day.value.assignments.get(sel)
                        && let Some(e) = challenges::exercise(&a.record_id)
                    {
                        rows.push(String::new());
                        rows.push(format!("{:?} mode · {}", e.mode, e.instructions));
                        if let Some(c) = &e.measured {
                            let value = p
                                .snapshot
                                .lifetime(c.metric, Some(e.mode), a.include_footnotes)
                                .zip(a.baseline)
                                .map(|(a, b)| a - b);
                            rows.push(format!("Measured criterion: {} / {} {} since acceptance (Ctrl-S checkpoints pending edits)",value.map_or("not accepted/unavailable".into(),|n|n.to_string()),c.target,c.metric.label()));
                        } else {
                            rows.push("Subjective exercise: x explicitly self-reports completion; no automatic judgment of your prose or reading.".into());
                        }
                    }
                    if day.value.assignments.is_empty() {
                        rows.push("No exercise matches saved filters. Widen future filters; no hidden repeated or invented assignment.".into());
                    }
                } else {
                    rows.push(
                        if p.config.challenges {
                            "No readable assignment for this date. Originals retained. \
                             r reloads and → returns toward today."
                        } else {
                            "Challenges disabled. Open Tools › Progress › Edits… and press d \
                             to enable them."
                        }
                        .into(),
                    );
                }
            }
            Page::ChallengeFilters => {
                if let Some(profile) = &p.draft_profile {
                    selected = Some(rows.len() + sel);
                    for category in Category::ALL {
                        rows.push(format!(
                            "[{}] {}",
                            if profile.categories.contains(&category) {
                                'x'
                            } else {
                                ' '
                            },
                            category.label()
                        ));
                    }
                    rows.push(format!("Mode {:?} (None = mixed)", profile.mode));
                    rows.push(format!("Maximum difficulty {}", profile.difficulty));
                    rows.push(format!("Maximum duration {} min", profile.max_minutes));
                    rows.push("Enter saves for future dates and explicit rerolls only; today's existing tasks do not silently regenerate.".into());
                }
            }
        }
        self.rows = rows;
        self.row_selection = selected;
    }
}
