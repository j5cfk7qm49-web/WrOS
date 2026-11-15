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
    /// Connected, but the kernel doesn't report the signal (no `/proc/net/wireless`).
    Connected,
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

/// The signal now, or `None` where there is no Wi-Fi at all. Without `/proc/net/wireless`
/// (a kernel built without the wireless-extensions file), the interfaces in `/sys/class/net`
/// still say whether one is connected (#128).
#[cfg(target_os = "linux")]
pub fn read() -> Option<Wifi> {
    match std::fs::read_to_string("/proc/net/wireless") {
        Ok(text) => Some(parse(&text)),
        Err(_) => from_sysfs(std::path::Path::new("/sys/class/net")),
    }
}

/// From a `/sys/class/net`-like directory: [`Wifi::Connected`] when a wireless interface is up,
/// [`Wifi::Off`] when there are wireless interfaces but none is up, `None` without any.
pub fn from_sysfs(net: &std::path::Path) -> Option<Wifi> {
    let mut wireless = false;
    for entry in std::fs::read_dir(net).ok()?.flatten() {
        let dir = entry.path();
        if !(dir.join("wireless").is_dir() || dir.join("phy80211").exists()) {
            continue;
        }
        wireless = true;
        let up = std::fs::read_to_string(dir.join("operstate")).is_ok_and(|s| s.trim() == "up");
        if up {
            return Some(Wifi::Connected);
        }
    }
    wireless.then_some(Wifi::Off)
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

    /// A `/sys/class/net` with the given interfaces: (name, wireless, operstate).
    fn net(tag: &str, ifaces: &[(&str, bool, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("wros-wifi-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (name, wireless, state) in ifaces {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            if *wireless {
                std::fs::create_dir_all(dir.join("wireless")).unwrap();
            }
            std::fs::write(dir.join("operstate"), format!("{state}\n")).unwrap();
        }
        root
    }

    #[test]
    fn without_proc_net_wireless_the_mark_says_connected_or_off() {
        // #128: when the kernel has no /proc/net/wireless, the mark disappeared. The interface's
        // own state says connected (strength unknown) or not.
        let up = net("up", &[("lo", false, "unknown"), ("wlan0", true, "up")]);
        assert_eq!(from_sysfs(&up), Some(Wifi::Connected));
        let down = net("down", &[("wlp1s0", true, "down")]);
        assert_eq!(from_sysfs(&down), Some(Wifi::Off));
        let wired = net("wired", &[("lo", false, "unknown"), ("eth0", false, "up")]);
        assert_eq!(from_sysfs(&wired), None, "no Wi-Fi hardware: no mark");
        assert_eq!(from_sysfs(std::path::Path::new("/nonexistent/net")), None);
        for root in [up, down, wired] {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn wifi_without_an_interface_is_off() {
        assert_eq!(parse(HEAD), Wifi::Off);
        assert_eq!(parse(""), Wifi::Off);
        assert_eq!(parse("garbage\n\n wlan0: x y z\n"), Wifi::Off);
    }
}
