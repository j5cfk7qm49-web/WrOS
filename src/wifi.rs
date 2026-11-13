//! Wi-Fi signal for the status bar, from the kernel's `/proc/net/wireless`:
//! the connected interface's link quality, read on the gauges' own thread at
//! most every 30 seconds. It never scans and never runs a command.

/// What the status bar shows for Wi-Fi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wifi {
    /// No wireless interface, the radio off, or not connected.
    Off,
    /// Connected, with one to three bars of signal.
    Bars(u8),
}

/// The link quality most drivers report at full signal.
const FULL: f32 = 70.0;

/// Read `/proc/net/wireless`'s text: the first interface's link quality.
pub fn parse(text: &str) -> Wifi {
    let Some(line) = text.lines().skip(2).find(|l| l.contains(':')) else {
        return Wifi::Off;
    };
    let fields: Vec<&str> = line
        .split_once(':')
        .map_or("", |(_, rest)| rest)
        .split_whitespace()
        .collect();
    let link = fields
        .get(1)
        .and_then(|f| f.trim_end_matches('.').parse::<f32>().ok())
        .unwrap_or(0.0);
    match link / FULL {
        q if q <= 0.0 => Wifi::Off,
        q if q < 1.0 / 3.0 => Wifi::Bars(1),
        q if q < 2.0 / 3.0 => Wifi::Bars(2),
        _ => Wifi::Bars(3),
    }
}

/// The signal now, or `None` where the kernel doesn't report Wi-Fi at all.
#[cfg(target_os = "linux")]
pub fn read() -> Option<Wifi> {
    std::fs::read_to_string("/proc/net/wireless")
        .ok()
        .map(|text| parse(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE\n face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22\n";

    #[test]
    fn wifi_parses_link_quality_into_three_bars() {
        let at = |link: &str| {
            parse(&format!(
                "{HEAD} wlan0: 0000   {link}  -56.  -256        0      0      0      0     35        0\n"
            ))
        };
        assert_eq!(at("70."), Wifi::Bars(3));
        assert_eq!(at("54."), Wifi::Bars(3));
        assert_eq!(at("40."), Wifi::Bars(2));
        assert_eq!(at("12."), Wifi::Bars(1));
        assert_eq!(at("0."), Wifi::Off, "an interface that isn't connected");
    }

    #[test]
    fn wifi_without_an_interface_is_off() {
        assert_eq!(parse(HEAD), Wifi::Off);
        assert_eq!(parse(""), Wifi::Off);
        assert_eq!(parse("garbage\n\n wlan0: x y z\n"), Wifi::Off);
    }
}
