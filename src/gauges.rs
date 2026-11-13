//! Wi-Fi, volume, battery and the clock at the right of the bottom bar (#81),
//! and the screen light for a moment after its key. Read every 30 seconds on a
//! thread of its own, so a slow sound helper never holds up typing; Settings
//! updates them at once when it changes one, and the brightness and volume keys
//! (#80) within half a second. The graphical session shows symbols; the text
//! console, whose font has few of them, shows words.

pub use crate::wifi::Wifi;

/// Below this, the battery shows even in focus mode.
pub const LOW_BATTERY: u8 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gauges {
    pub battery: Option<u8>,
    pub charging: bool,
    pub light: Option<u8>,
    pub volume: Option<u8>,
    pub muted: bool,
    /// `None` where the kernel doesn't report Wi-Fi at all.
    pub wifi: Option<Wifi>,
}

/// One thing at the right of the bottom bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Wifi,
    Volume,
    Light,
    Battery,
    Clock,
}

/// How long the light shows after a brightness key.
pub const LIGHT_SHOWN: std::time::Duration = std::time::Duration::from_secs(4);

impl Gauges {
    pub fn low_battery(&self) -> bool {
        !self.charging && self.battery.is_some_and(|b| b < LOW_BATTERY)
    }

    /// The right of the bottom bar, left to right: Wi-Fi, volume, the light
    /// (only while `light` says a brightness key was just pressed), battery
    /// and the clock. Focus mode shows only a low battery. `console` spells
    /// them in words for the text console's font.
    pub fn items(
        &self,
        focus: bool,
        console: bool,
        light: bool,
        clock: &str,
    ) -> Vec<(Item, String)> {
        let battery = |b: u8| {
            let plus = if self.charging { "+" } else { "" };
            if console {
                format!("Bat {b}%{plus}")
            } else {
                format!("▮{b}%{plus}")
            }
        };
        if focus {
            return match self.battery {
                Some(b) if self.low_battery() => vec![(Item::Battery, battery(b))],
                _ => vec![],
            };
        }
        let mut out = vec![];
        if let Some(wifi) = self.wifi {
            out.push((
                Item::Wifi,
                match (wifi, console) {
                    (Wifi::Off, true) => "Wi-Fi off".into(),
                    (Wifi::Bars(n), true) => format!("Wi-Fi {n}/3"),
                    (Wifi::Off, false) => "⊘".into(),
                    (Wifi::Bars(_), false) => "▂▄▆".into(),
                },
            ));
        }
        if self.muted {
            out.push((
                Item::Volume,
                if console { "Vol muted" } else { "♪×" }.into(),
            ));
        } else if let Some(v) = self.volume {
            out.push((
                Item::Volume,
                if console {
                    format!("Vol {v}%")
                } else {
                    format!("♪{v}%")
                },
            ));
        }
        if light && let Some(l) = self.light {
            out.push((
                Item::Light,
                if console {
                    format!("Light {l}%")
                } else {
                    format!("☀{l}%")
                },
            ));
        }
        if let Some(b) = self.battery {
            out.push((Item::Battery, battery(b)));
        }
        if !clock.is_empty() {
            out.push((Item::Clock, clock.to_string()));
        }
        out
    }
}

/// Keep what fits in `room` cells (each item and a gap of two): the clock goes
/// first on a narrow screen, then volume, then Wi-Fi; the battery last.
pub fn fit(items: Vec<(Item, String)>, room: usize) -> Vec<(Item, String)> {
    let mut keep = items;
    let width = |items: &[(Item, String)]| -> usize {
        items.iter().map(|(_, s)| s.chars().count() + 2).sum()
    };
    for drop in [
        Item::Clock,
        Item::Light,
        Item::Volume,
        Item::Wifi,
        Item::Battery,
    ] {
        if width(&keep) <= room {
            break;
        }
        keep.retain(|(item, _)| *item != drop);
    }
    keep
}

/// The gauges now. Battery and light come from the kernel's status files; the
/// volume from the WrOS sound helper, which needs no privileges.
#[cfg(target_os = "linux")]
pub fn read() -> Gauges {
    use wros_sys::{Backend, Reply, Request};
    let sys = std::path::Path::new("/sys");
    let (battery, charging) = wros_sys::linux::battery(sys);
    let mut gauges = Gauges {
        battery,
        charging: charging.unwrap_or(false),
        light: wros_sys::linux::brightness(sys),
        wifi: crate::wifi::read(),
        ..Gauges::default()
    };
    if let Ok(Reply::Audio(audio)) = wros_sys::linux::Linux::default().request(Request::AudioStatus)
        && audio.available
    {
        gauges.volume = audio.volume_percent;
        gauges.muted = audio.muted;
    }
    gauges
}

/// Touched by `/usr/libexec/wros-media-key` after a brightness or volume key (#80).
#[cfg(target_os = "linux")]
const KEYS_CHANGED: &str = "/run/wros-keys/changed";

/// True once when the key marker's time differs from the one last seen.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn marker_moved(
    seen: &mut Option<std::time::SystemTime>,
    now: Option<std::time::SystemTime>,
) -> bool {
    let moved = now != *seen;
    *seen = now;
    moved
}

/// Reads the gauges now and every 30 seconds after, until the receiver is gone.
/// A brightness or volume key reads them again within half a second.
#[cfg(target_os = "linux")]
pub fn watch() -> std::sync::mpsc::Receiver<Gauges> {
    use std::time::Duration;
    const EVERY: Duration = Duration::from_secs(30);
    const LOOK: Duration = Duration::from_millis(500);
    let modified = || {
        std::fs::metadata(KEYS_CHANGED)
            .and_then(|m| m.modified())
            .ok()
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut seen = modified();
        while tx.send(read()).is_ok() {
            let mut waited = Duration::ZERO;
            while waited < EVERY {
                std::thread::sleep(LOOK);
                waited += LOOK;
                if marker_moved(&mut seen, modified()) {
                    break;
                }
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: Vec<(Item, String)>) -> Vec<String> {
        items.into_iter().map(|(_, s)| s).collect()
    }

    #[test]
    fn the_bar_shows_symbols_and_the_console_words() {
        let gauges = Gauges {
            battery: Some(76),
            charging: true,
            light: Some(50),
            volume: Some(70),
            muted: false,
            wifi: Some(Wifi::Bars(2)),
        };
        assert_eq!(
            texts(gauges.items(false, false, false, "14:07")),
            ["▂▄▆", "♪70%", "▮76%+", "14:07"]
        );
        assert_eq!(
            texts(gauges.items(false, true, false, "14:07")),
            ["Wi-Fi 2/3", "Vol 70%", "Bat 76%+", "14:07"]
        );
        let muted = Gauges {
            charging: false,
            muted: true,
            wifi: Some(Wifi::Off),
            ..gauges
        };
        assert_eq!(
            texts(muted.items(false, false, false, "")),
            ["⊘", "♪×", "▮76%"]
        );
        assert!(Gauges::default().items(false, false, false, "").is_empty());
    }

    #[test]
    fn brightness_shows_briefly_after_its_key() {
        let gauges = Gauges {
            light: Some(40),
            battery: Some(90),
            ..Gauges::default()
        };
        assert_eq!(texts(gauges.items(false, false, false, "")), ["▮90%"]);
        assert_eq!(
            texts(gauges.items(false, false, true, "")),
            ["☀40%", "▮90%"]
        );
    }

    #[test]
    fn narrow_screens_drop_clock_then_volume_then_wifi() {
        let gauges = Gauges {
            battery: Some(76),
            volume: Some(70),
            wifi: Some(Wifi::Bars(3)),
            ..Gauges::default()
        };
        let all = gauges.items(false, false, false, "14:07");
        let order =
            |room| -> Vec<Item> { fit(all.clone(), room).into_iter().map(|(i, _)| i).collect() };
        assert_eq!(
            order(100),
            [Item::Wifi, Item::Volume, Item::Battery, Item::Clock]
        );
        assert_eq!(order(21), [Item::Wifi, Item::Volume, Item::Battery]);
        assert_eq!(order(14), [Item::Wifi, Item::Battery]);
        assert_eq!(order(8), [Item::Battery]);
        assert_eq!(order(2), Vec::<Item>::new());
    }

    #[test]
    fn a_key_press_is_noticed_once_and_a_missing_marker_is_not_a_press() {
        use std::time::{Duration, SystemTime};
        let mut seen = None;
        assert!(!marker_moved(&mut seen, None));
        let first = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        assert!(marker_moved(&mut seen, Some(first)));
        assert!(!marker_moved(&mut seen, Some(first)));
        assert!(marker_moved(
            &mut seen,
            Some(first + Duration::from_millis(1))
        ));
    }

    #[test]
    fn focus_mode_shows_only_a_low_battery_that_is_not_charging() {
        let low = Gauges {
            battery: Some(12),
            light: Some(50),
            ..Gauges::default()
        };
        assert_eq!(texts(low.items(true, false, false, "14:07")), ["▮12%"]);
        assert_eq!(texts(low.items(true, true, false, "14:07")), ["Bat 12%"]);
        assert!(
            Gauges {
                charging: true,
                ..low
            }
            .items(true, false, false, "")
            .is_empty()
        );
        assert!(
            Gauges {
                battery: Some(LOW_BATTERY),
                ..low
            }
            .items(true, false, false, "")
            .is_empty()
        );
    }
}
