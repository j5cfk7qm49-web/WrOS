use crate::app::{App, Overlay};
use wros_sys::Request;

impl App {
    pub(in crate::app) fn scan_widget_wifi(&mut self) {
        self.deck.widget_wifi_scan_pending = self.deck.shell.wifi_networks.is_none();
        self.run_widget_wifi_scan();
    }

    pub(super) fn run_widget_wifi_scan(&mut self) {
        if !matches!(
            self.overlay,
            Overlay::Widget {
                kind: crate::gauges::Widget::Wifi,
                ..
            }
        ) || self.deck.shell.wifi_networks.is_some()
        {
            self.deck.widget_wifi_scan_pending = false;
        }
        if self.deck.widget_wifi_scan_pending && !self.deck.busy {
            self.deck.widget_wifi_scan_pending = false;
            self.deck.request(Request::ScanWifi);
        }
    }

    pub(super) fn replace_wifi_networks(&mut self, networks: Option<Vec<wros_sys::WifiNetwork>>) {
        let rows = |networks: &Option<Vec<wros_sys::WifiNetwork>>| {
            networks
                .as_ref()
                .map_or(1, |networks| networks.len().max(1))
        };
        if let Overlay::Widget {
            kind: crate::gauges::Widget::Wifi,
            sel,
        } = &mut self.overlay
        {
            // Keep the toggle and Settings selected across an asynchronous scan.
            let old = rows(&self.deck.shell.wifi_networks);
            let new = rows(&networks);
            *sel = if *sel >= old {
                new + (*sel - old)
            } else {
                (*sel).min(new - 1)
            };
        }
        self.deck.shell.wifi_networks = networks;
    }
}
