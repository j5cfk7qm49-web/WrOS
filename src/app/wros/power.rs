use super::{Deck, PendingPower, PowerWait};
use crate::app::App;
use grimoire_core::tutorial;
use std::time::Duration;
use wros_sys::{PowerAction, Request};

impl PowerWait {
    fn action(self) -> PowerAction {
        match self {
            PowerWait::Shutdown => PowerAction::Shutdown,
            PowerWait::Sleep => PowerAction::Suspend,
            PowerWait::Restart | PowerWait::UpdateRestart => PowerAction::Restart,
        }
    }
    fn noun(self) -> &'static str {
        match self {
            PowerWait::Shutdown => "Shutting down",
            PowerWait::Sleep => "Going to sleep",
            PowerWait::Restart => "Restarting",
            PowerWait::UpdateRestart => "Restarting to install the update",
        }
    }
    fn refused(self) -> &'static str {
        match self {
            PowerWait::Restart | PowerWait::Shutdown | PowerWait::Sleep => "Power unchanged:",
            PowerWait::UpdateRestart => "Not restarting:",
        }
    }
}

/// Give up on a waiting restart or shutdown after this long (an offline sync that hangs).
const POWER_WAIT_LIMIT: Duration = Duration::from_secs(10 * 60);

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

impl Deck {
    /// "Restarting when the sync finishes · 0:12 · Esc cancels" while a power action waits.
    pub(in crate::app) fn power_wait_line(&self) -> Option<String> {
        let pending = self.power_wait?;
        let time = wros_sys::elapsed_label(pending.since.elapsed())
            .map(|time| format!(" · {time}"))
            .unwrap_or_default();
        Some(format!(
            "{} when {} finishes{time} · Esc cancels",
            pending.kind.noun(),
            pending.what
        ))
    }
}

impl App {
    pub(crate) fn request_suspend(&mut self) -> bool {
        if !self.deck.suspend_available || !self.deck.lock_available || self.deck.live_session {
            self.deck.note = wros_sys::SUSPEND_UNAVAILABLE.into();
            self.deck.shell.notices.push(&self.deck.note);
            return true;
        }
        // A sync never holds sleep up: the lock never waits for the network (#168).
        if let Some(what) = self.sleep_blocker() {
            self.deck.note = format!(
                "{} is running. Wait for it to finish, then try again.",
                capitalise(what)
            );
            self.deck.shell.notices.push(&self.deck.note);
            return false;
        }
        self.request_console_lock();
        if self.deck.lock_requested {
            // Locking never waits for the network: start the final sync and lock now. If
            // sleep interrupts it, it finishes after wake.
            self.start_final_sync();
            self.deck.suspend_requested = true;
        }
        true
    }

    /// What a closed lid or a nearly empty battery asks for right now: nothing,
    /// a lock (`Some(false)`), or a lock and sleep (`Some(true)`). The battery
    /// warns first and counts down before it asks for sleep, whatever the lid choice.
    pub(crate) fn power_event(
        &mut self,
        lid_closed: bool,
        battery: crate::battery::Reading,
        now: std::time::Instant,
    ) -> Option<bool> {
        use crate::battery::Step;
        let shown = self.deck.battery.showing(now).map(str::to_string);
        let step = self
            .deck
            .battery
            .read(battery, self.deck.suspend_available, now);
        // The notices keep each warning, and a countdown once, not every second of it.
        if let Some(said) = self.deck.battery.showing(now)
            && shown.as_deref() != Some(said)
            && (step == Step::Save || !said.contains(" seconds"))
        {
            self.deck.shell.notices.push(said);
            if battery.discharging {
                self.observe_deck(tutorial::Event::BatteryWarningShown);
            }
        }
        match step {
            Step::Sleep => return Some(true),
            Step::Save => {
                self.commit_saves();
            }
            Step::Nothing => {}
        }
        if !lid_closed || !self.deck.lock_available {
            return None;
        }
        match self.deck.lid_close {
            crate::lid::LidClose::Nothing => None,
            crate::lid::LidClose::Lock => Some(false),
            crate::lid::LidClose::LockAndSleep => Some(self.deck.suspend_available),
        }
    }

    /// Acts on `power_event`. False means the event must be retried after
    /// the current system or backup operation finishes.
    pub(crate) fn request_power_lock(&mut self, sleep: bool) -> bool {
        if sleep {
            return self.request_suspend();
        }
        if self.deck.busy {
            return false;
        }
        self.request_console_lock();
        true
    }

    pub(in crate::app) fn cycle_lid_close(&mut self) {
        let Some(settings) = self.deck.lid_settings.clone() else {
            return;
        };
        let next = self.deck.lid_close.next();
        let saved = crate::lid::save(&settings, next);
        self.deck.note = match &saved {
            Ok(()) => {
                self.deck.lid_close = next;
                match next {
                    crate::lid::LidClose::LockAndSleep => {
                        "Closing the lid now saves, locks and sleeps."
                    }
                    crate::lid::LidClose::Lock => {
                        "Closing the lid now saves and locks. The laptop stays awake."
                    }
                    crate::lid::LidClose::Nothing => {
                        "Closing the lid now does nothing. Your project stays open."
                    }
                }
                .to_string()
            }
            Err(error) => format!("Couldn't save the lid choice: {error}"),
        };
        self.deck.shell.notices.push(&self.deck.note);
        if saved.is_ok() {
            self.observe_deck(tutorial::Event::LidChoiceSaved);
        }
    }

    /// What sleep waits for: a system request or a backup, never a sync.
    pub(super) fn sleep_blocker(&mut self) -> Option<&'static str> {
        self.deck.shell.backup.tick();
        if self.deck.busy {
            Some("the system request")
        } else if self.deck.shell.backup.job.is_some() {
            Some("the backup")
        } else {
            None
        }
    }

    /// What has to finish before WrOS can restart or shut down, if anything.
    fn power_blocker(&mut self) -> Option<&'static str> {
        self.deck.shell.backup.tick();
        if self.deck.busy {
            Some("the system request")
        } else if self.deck.shell.backup.job.is_some() {
            Some("the backup")
        } else if self.deck.shell.backup.sync.job.is_some() {
            Some("the sync")
        } else if self.deck.shell.backup.shelved_sync_running().is_some() {
            Some("the previous project's sync")
        } else {
            None
        }
    }

    /// Restart or shut down (#168): save, close this writing session, sync once, then go.
    /// Anything still running is waited for, not refused: the action is kept and tried
    /// again, every check included, once it finishes. Esc cancels the wait.
    /// False when refused (nothing will happen); true when sent or waiting.
    pub(in crate::app) fn power_off(&mut self, kind: PowerWait) -> bool {
        self.deck.power_wait = None;
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.power_notice(format!(
                "{} unsaved buffers or source conflicts need attention. Return to writing to review them.",
                kind.refused()
            ));
            return false;
        }
        if kind == PowerWait::UpdateRestart && !self.update_staged_confirmed() {
            self.power_notice(
                "Not restarting: the system has not confirmed a staged update. Refresh Recovery status first.",
            );
            return false;
        }
        if let Some(what) = self.power_blocker() {
            self.power_hold(kind, what);
            return true;
        }
        self.save_deck();
        self.end_timeline_session(chrono::Local::now());
        if !self.final_sync_done() {
            // The final sync started; it is what we wait for now.
            self.power_hold(kind, "the final sync");
            return true;
        }
        self.deck.shell.update_flow.last_result = None;
        self.deck.request(Request::Power(kind.action()));
        true
    }

    pub(super) fn power_hold(&mut self, kind: PowerWait, what: &'static str) {
        let since = match self.deck.power_wait {
            Some(PendingPower { since, .. }) => since,
            None => std::time::Instant::now(),
        };
        self.deck.power_wait = Some(PendingPower { kind, what, since });
        let line = self.deck.power_wait_line().unwrap_or_default();
        self.power_notice(line);
    }

    /// Said in Settings and in the writing status line.
    fn power_notice(&mut self, text: impl Into<String>) {
        self.deck.note = text.into();
        self.deck.shell.notices.push(&self.deck.note);
        self.msg = self.deck.note.clone();
    }

    /// Esc while a restart or shutdown waits: keep WrOS running.
    pub(super) fn cancel_power_wait(&mut self) -> bool {
        let Some(pending) = self.deck.power_wait.take() else {
            return false;
        };
        self.power_notice(format!(
            "{} cancelled. {} carries on.",
            pending.kind.noun(),
            capitalise(pending.what)
        ));
        true
    }

    /// Go ahead with a waiting restart or shutdown once nothing runs, or give up after a while.
    pub(super) fn tick_power_wait(&mut self) {
        let Some(pending) = self.deck.power_wait else {
            return;
        };
        if pending.since.elapsed() >= POWER_WAIT_LIMIT {
            self.deck.power_wait = None;
            self.power_notice(format!(
                "{} cancelled: {} did not finish in {} minutes. Try again, or check Backups and accounts.",
                pending.kind.noun(),
                pending.what,
                POWER_WAIT_LIMIT.as_secs() / 60
            ));
            return;
        }
        if pending.kind == PowerWait::Sleep {
            if self.sleep_blocker().is_none() {
                self.deck.power_wait = None;
                self.request_suspend();
            }
            return;
        }
        if self.project.dirty_count() != 0 {
            // The writer went back to writing: their words come first.
            self.deck.power_wait = None;
            self.power_notice(format!(
                "{} cancelled: you went back to writing.",
                pending.kind.noun()
            ));
            return;
        }
        // A final sync that brought files from Drive leaves the book to be re-read first
        // (sync_tick does it); going ahead before then would refuse as 'unsaved'.
        if self.power_blocker().is_none() && !self.tree_stale {
            self.power_off(pending.kind);
            // Still waiting (a final sync started): keep the original start time.
            if let Some(waiting) = &mut self.deck.power_wait {
                waiting.since = pending.since;
            }
        }
    }

    pub(crate) fn request_console_lock(&mut self) {
        if self.deck.live_session {
            self.deck
                .shell
                .notices
                .push("Live sessions have no personal screen lock.");
            return;
        }
        if !self.deck.lock_available || self.deck.busy {
            self.deck
                .shell
                .notices
                .push("Console locking is unavailable or a system request is pending.");
            return;
        }
        if !self.commit_saves()
            || self.project.dirty_count() != 0
            || self.tree_stale
            || !self.conflicts().is_empty()
        {
            self.deck
                .shell
                .notices
                .push("Lock not started: unsaved buffers or source conflicts need attention.");
            return;
        }
        self.save_deck();
        self.end_timeline_session(chrono::Local::now());
        self.deck.lock_requested = true;
    }

    /// The console lock handed the session back after the writer's password.
    pub(crate) fn on_unlocked(&mut self) {
        self.deck.note = self
            .deck
            .battery
            .unlocked()
            .unwrap_or("Writing session unlocked.")
            .into();
        if self.deck.note != "Writing session unlocked." {
            self.msg = self.deck.note.clone();
        }
        self.deck.shell.notices.push(&self.deck.note);
        self.observe_deck(tutorial::Event::SessionUnlocked);
    }
}
