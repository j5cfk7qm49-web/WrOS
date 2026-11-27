//! Rows for the Battery and power Settings page.

use super::{App, Choice, Entry};

pub(super) fn power_panel_rows(app: &App, rows: &mut Vec<Entry>) {
    let status = app.deck.snapshot.as_ref();
    rows.extend([
        Entry::info(format!(
            "Battery: {}",
            status
                .and_then(|s| s.battery_percent)
                .map_or("unavailable".into(), |p| format!("{p}%"))
        )),
        Entry::new(
            format!(
                "Low-power interface and OS request: {}",
                if app.deck.state.low_power {
                    "on"
                } else {
                    "off"
                }
            ),
            Choice::System(7),
        ),
        if app.deck.suspend_available {
            Entry::new("Lock and suspend", Choice::System(8))
        } else {
            Entry::info(wros_sys::SUSPEND_UNAVAILABLE)
        },
    ]);
    // Above Shutdown, so Lock writing session stays the last row.
    if app.deck.lock_available && app.deck.lid_settings.is_some() {
        rows.push(Entry::new(
            format!("When the lid closes: {}", app.deck.lid_close.label()),
            Choice::LidClose,
        ));
        if app.deck.suspend_available {
            rows.push(Entry::info(
                "On battery, WrOS warns at 20% and 10%. At 8% it saves, then locks and sleeps after 30 seconds.",
            ));
        }
    }
    if cfg!(unix) {
        rows.push(Entry::new(
            "Restart the writing app · you stay signed in",
            Choice::RestartApp,
        ));
    }
    rows.extend([
        Entry::new("Shutdown · confirmation required", Choice::System(9)),
        Entry::new("Restart · confirmation required", Choice::System(10)),
        if app.deck.live_session {
            Entry::info("Live session · personal lock unavailable")
        } else if app.deck.lock_available {
            Entry::new(
                if app.deck.busy {
                    "Lock writing session · waiting for the current task"
                } else {
                    "Lock writing session"
                },
                Choice::Lock,
            )
        } else if app.deck.graphical_session {
            Entry::info("Lock and sleep · not in the graphical session yet")
        } else {
            Entry::info("Console lock unavailable in this image/session")
        },
    ]);
}
