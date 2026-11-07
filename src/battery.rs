//! Running on battery: warnings at 20% and 10%, then at the reserve (8%) a
//! countdown before WrOS saves, locks and sleeps. Everything WrOS does on its
//! own is said first, in the status bar and in Settings' notices.

use std::time::{Duration, Instant};

/// Where WrOS saves, locks and sleeps.
pub const RESERVE: u8 = 8;
/// The warnings before that, highest first.
pub const WARNINGS: [u8; 2] = [20, 10];
/// The countdown before sleeping; longer after a sleep, to plug in or finish a sentence.
pub const GRACE: Duration = Duration::from_secs(30);
pub const GRACE_AFTER_SLEEP: Duration = Duration::from_secs(120);

/// One reading of the battery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    pub percent: Option<u8>,
    pub discharging: bool,
    /// The battery says it is at its reserve, or the percent is.
    pub critical: bool,
}

/// What the battery watch remembers between readings.
#[derive(Debug, Default)]
pub struct Watch {
    /// The lowest warning already given on this discharge.
    warned: Option<u8>,
    /// When the reserve countdown started, and how long it runs.
    countdown: Option<(Instant, Duration)>,
    /// The last sleep was the battery's, so the unlock says why.
    pub slept: bool,
    /// The battery slept the laptop since it was last charging: the next countdown is longer.
    after_sleep: bool,
    /// What to show while it matters, and until when.
    pub notice: Option<(String, Instant)>,
}

/// What the watch wants done after a reading.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Save the words now: the countdown has started.
    Save,
    /// Save, lock and sleep.
    Sleep,
}

impl Watch {
    /// Takes a reading; `can_sleep` is whether this computer can sleep here.
    pub fn read(&mut self, reading: Reading, can_sleep: bool, now: Instant) -> Step {
        if !reading.discharging {
            if self.countdown.take().is_some() {
                self.say("Charging. WrOS stays awake.", now, 10);
            }
            self.warned = None;
            self.after_sleep = false;
            return Step::Nothing;
        }
        if reading.percent.is_some_and(|p| p > WARNINGS[0] + 5) && !reading.critical {
            self.warned = None;
        }
        if reading.critical {
            let percent = reading.percent.unwrap_or(RESERVE);
            if !can_sleep {
                if self.warned != Some(0) {
                    self.warned = Some(0);
                    self.say(
                        &format!(
                            "Battery {percent}%. This computer can't sleep from WrOS. Plug in now; your words are saved."
                        ),
                        now,
                        600,
                    );
                    return Step::Save;
                }
                return Step::Nothing;
            }
            let (start, grace) = *self.countdown.get_or_insert((
                now,
                if self.after_sleep {
                    GRACE_AFTER_SLEEP
                } else {
                    GRACE
                },
            ));
            let left = grace.saturating_sub(now - start);
            if left.is_zero() {
                self.countdown = None;
                self.slept = true;
                self.after_sleep = true;
                return Step::Sleep;
            }
            self.say(
                &format!(
                    "Battery {percent}%. WrOS saves, locks and sleeps in {} seconds. Plug in to stay awake.",
                    left.as_secs().max(1)
                ),
                now,
                5,
            );
            return if now == start {
                Step::Save
            } else {
                Step::Nothing
            };
        }
        self.countdown = None;
        let Some(percent) = reading.percent else {
            return Step::Nothing;
        };
        for level in WARNINGS {
            if percent <= level && self.warned.is_none_or(|w| w > level) {
                // Only the lowest warning that applies: waking at 9% doesn't say 20% first.
                if WARNINGS
                    .iter()
                    .any(|&lower| lower < level && percent <= lower)
                {
                    continue;
                }
                self.warned = Some(level);
                let urgency = if level == WARNINGS[0] {
                    "Plug in soon."
                } else {
                    "Plug in now."
                };
                self.say(
                    &format!(
                        "Battery {percent}%. {urgency} At {RESERVE}% WrOS saves, locks and sleeps."
                    ),
                    now,
                    120,
                );
            }
        }
        Step::Nothing
    }

    /// The notice to show now, if any.
    pub fn showing(&self, now: Instant) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, until)| now < *until)
            .map(|(text, _)| text.as_str())
    }

    fn say(&mut self, text: &str, now: Instant, seconds: u64) {
        self.notice = Some((text.to_string(), now + Duration::from_secs(seconds)));
    }

    /// The unlock message, once: why WrOS slept, when it was the battery.
    pub fn unlocked(&mut self) -> Option<&'static str> {
        std::mem::take(&mut self.slept).then_some(
            "WrOS slept because the battery reached its reserve. Plug in before writing on.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(percent: u8) -> Reading {
        Reading {
            percent: Some(percent),
            discharging: true,
            critical: percent <= RESERVE,
        }
    }

    #[test]
    fn warns_at_twenty_and_ten_once_each_and_names_what_happens_at_eight() {
        let mut watch = Watch::default();
        let t = Instant::now();
        assert_eq!(watch.read(on(35), true, t), Step::Nothing);
        assert_eq!(watch.showing(t), None);
        watch.read(on(20), true, t);
        let said = watch.showing(t).unwrap().to_string();
        assert!(
            said.contains("Battery 20%") && said.contains("At 8% WrOS saves, locks and sleeps")
        );
        watch.notice = None;
        watch.read(on(19), true, t);
        assert_eq!(watch.showing(t), None, "20% is said once");
        watch.read(on(10), true, t);
        assert!(
            watch
                .showing(t)
                .unwrap()
                .contains("Battery 10%. Plug in now.")
        );
        // A wake at 9% after charging a little says 10%, not 20% first.
        let mut woke = Watch::default();
        woke.read(on(9), true, t);
        assert!(
            woke.showing(t)
                .unwrap()
                .contains("Battery 9%. Plug in now.")
        );
    }

    #[test]
    fn the_reserve_counts_down_saving_first_and_charging_cancels_it() {
        let mut watch = Watch::default();
        let t = Instant::now();
        assert_eq!(watch.read(on(8), true, t), Step::Save);
        assert!(watch.showing(t).unwrap().contains("sleeps in 30 seconds"));
        let later = t + Duration::from_secs(12);
        assert_eq!(watch.read(on(8), true, later), Step::Nothing);
        assert!(watch.showing(later).unwrap().contains("in 18 seconds"));
        let plugged = Reading {
            discharging: false,
            ..on(8)
        };
        assert_eq!(watch.read(plugged, true, later), Step::Nothing);
        assert_eq!(watch.showing(later), Some("Charging. WrOS stays awake."));
        // Unplugged again: a new countdown, then sleep, then the unlock says why.
        let again = later + Duration::from_secs(1);
        assert_eq!(watch.read(on(8), true, again), Step::Save);
        assert_eq!(watch.read(on(7), true, again + GRACE), Step::Sleep);
        assert!(watch.unlocked().unwrap().contains("battery"));
        assert_eq!(watch.unlocked(), None);
    }

    #[test]
    fn after_a_battery_sleep_the_next_countdown_is_longer() {
        let mut watch = Watch::default();
        let t = Instant::now();
        watch.read(on(8), true, t);
        assert_eq!(watch.read(on(8), true, t + GRACE), Step::Sleep);
        assert!(watch.unlocked().is_some());
        let woke = t + Duration::from_secs(600);
        assert_eq!(watch.read(on(7), true, woke), Step::Save);
        assert!(watch.showing(woke).unwrap().contains("in 120 seconds"));
        assert_eq!(watch.read(on(7), true, woke + GRACE), Step::Nothing);
        assert_eq!(
            watch.read(on(7), true, woke + GRACE_AFTER_SLEEP),
            Step::Sleep
        );
    }

    #[test]
    fn without_sleep_the_reserve_saves_once_and_says_so() {
        let mut watch = Watch::default();
        let t = Instant::now();
        assert_eq!(watch.read(on(6), false, t), Step::Save);
        assert!(watch.showing(t).unwrap().contains("can't sleep"));
        assert_eq!(watch.read(on(5), false, t), Step::Nothing);
    }
}
