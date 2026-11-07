//! Battery, screen light and volume at the right of the bottom bar (#81). Read
//! every 30 seconds on a thread of its own, so a slow sound helper never holds
//! up typing; Settings updates them at once when it changes one.

/// Below this, the battery shows even in focus mode.
pub const LOW_BATTERY: u8 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gauges {
    pub battery: Option<u8>,
    pub charging: bool,
    pub light: Option<u8>,
    pub volume: Option<u8>,
    pub muted: bool,
}

impl Gauges {
    pub fn low_battery(&self) -> bool {
        !self.charging && self.battery.is_some_and(|b| b < LOW_BATTERY)
    }

    /// `Bat 76%  Light 50%  Vol 70%`, in words because the console font may lack
    /// symbols. `+` after the battery while charging. Focus mode shows only a
    /// low battery.
    pub fn text(&self, focus: bool) -> String {
        if focus {
            return if self.low_battery() {
                format!("Bat {}%", self.battery.unwrap_or_default())
            } else {
                String::new()
            };
        }
        let mut parts = vec![];
        if let Some(b) = self.battery {
            parts.push(format!("Bat {b}%{}", if self.charging { "+" } else { "" }));
        }
        if let Some(l) = self.light {
            parts.push(format!("Light {l}%"));
        }
        if self.muted {
            parts.push("Vol muted".into());
        } else if let Some(v) = self.volume {
            parts.push(format!("Vol {v}%"));
        }
        parts.join("  ")
    }
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

/// Reads the gauges now and every 30 seconds after, until the receiver is gone.
#[cfg(target_os = "linux")]
pub fn watch() -> std::sync::mpsc::Receiver<Gauges> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while tx.send(read()).is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_says_what_it_knows_in_words() {
        let gauges = Gauges {
            battery: Some(76),
            charging: true,
            light: Some(50),
            volume: Some(70),
            muted: false,
        };
        assert_eq!(gauges.text(false), "Bat 76%+  Light 50%  Vol 70%");
        assert_eq!(gauges.text(true), "");
        let muted = Gauges {
            charging: false,
            muted: true,
            ..gauges
        };
        assert_eq!(muted.text(false), "Bat 76%  Light 50%  Vol muted");
        assert_eq!(Gauges::default().text(false), "");
        let desktop = Gauges {
            light: Some(40),
            ..Gauges::default()
        };
        assert_eq!(desktop.text(false), "Light 40%");
    }

    #[test]
    fn focus_mode_shows_only_a_low_battery_that_is_not_charging() {
        let low = Gauges {
            battery: Some(12),
            light: Some(50),
            ..Gauges::default()
        };
        assert_eq!(low.text(true), "Bat 12%");
        assert_eq!(
            Gauges {
                charging: true,
                ..low
            }
            .text(true),
            ""
        );
        assert_eq!(
            Gauges {
                battery: Some(LOW_BATTERY),
                ..low
            }
            .text(true),
            ""
        );
    }
}
