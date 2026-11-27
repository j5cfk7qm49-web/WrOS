//! Render Settings, including the Google Drive phone-link screen.

#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::navigation::breadcrumb;
use super::pages::{entries, kinds};
use super::{App, Route};
use crate::geometry::to_u16;
use crate::theme::Theme;
use ratatui::layout::Rect;

pub(in crate::app) fn draw(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    if app.deck.shell.nav.current() == Route::LinkPhone
        && let Some(offer) = app
            .deck
            .shell
            .backup
            .job
            .as_ref()
            .and_then(|job| job.offer())
        && draw_link_offer(frame, area, &offer, app.linux_console)
    {
        // The QR page fills the screen and has no rows: clicks are judged against it, not
        // against the box and rows of the page before it (#124).
        app.settings_area.set(area);
        app.settings_hits.borrow_mut().clear();
        return;
    }
    let entries = entries(app);
    let kinds = kinds(app, &entries);
    let rows = entries
        .into_iter()
        .map(|entry| entry.label)
        .collect::<Vec<_>>();
    let route = app.deck.shell.nav.current();
    let mut title = breadcrumb(route).unwrap_or_else(|| match route {
        Route::WarmScreen => "WrOS · Settings › Display › Warm screen".into(),
        Route::WarmPreview => "WrOS · Keep display preview?".into(),
        Route::Timezones => "WrOS · Settings › Date and time › Timezone".into(),
        Route::TimezoneConfirm => "WrOS · Change timezone?".into(),
        Route::WifiNetworks => "WrOS · Settings › Wi-Fi networks".into(),
        Route::MusicServerForm => format!("WrOS · Set up {} server", app.music.source.label()),
        Route::WifiConnect => app
            .deck
            .shell
            .wifi_form
            .as_ref()
            .map(|f| format!("WrOS · Connect Wi-Fi › {}", f.network.ssid))
            .unwrap_or_else(|| "WrOS · Connect Wi-Fi".into()),
        Route::SavedWifi => "WrOS · Settings › Saved Wi-Fi profiles".into(),
        Route::SavedWifiProfile => "WrOS · Saved Wi-Fi profile".into(),
        Route::GoogleDisconnectConfirm => "WrOS · Disconnect Google Drive?".into(),
        Route::GoogleDrive => "WrOS · Settings › Google Drive".into(),
        Route::LinkPhone => "WrOS · Link Google Drive".into(),
        Route::SyncPanel => "WrOS · Settings › Synchronization".into(),
        Route::BackupForm => app
            .deck
            .shell
            .backup
            .form
            .as_ref()
            .map(|form| format!("WrOS · {}", form.title()))
            .unwrap_or_else(|| "WrOS · Backups".into()),
        Route::UpdatesView => "WrOS · Settings › Cached package plan".into(),
        Route::UpdateBundleForm => "WrOS · Review signed offline update".into(),
        Route::UpdateReview => "WrOS · Review signed update".into(),
        Route::UpdateConfirm => "WrOS · Stage this update?".into(),
        Route::UpdateStaged => "WrOS · Update staged".into(),
        Route::GoBackConfirm => "WrOS · Go back a version".into(),
        Route::RecoveryView => "WrOS · Settings › Recovery".into(),
        Route::RepairConfirm => "WrOS · Repair interrupted update?".into(),
        Route::StorageDevicesView => "WrOS · Settings › Mounted filesystems".into(),
        Route::InstallerConfirm => "WrOS · Start encrypted live installer?".into(),
        Route::ManualClock => "WrOS · Settings › Date and time › Manual clock".into(),
        Route::ManualClockConfirm => "WrOS · Set local clock?".into(),
        Route::KeyboardWizard => {
            "WrOS · Settings › Look and keyboard › Keyboard layout wizard".into()
        }
        // Every other page is on the menu map and has a breadcrumb.
        _ => "WrOS · Settings".into(),
    });
    if app.deck.live_session {
        title.push_str(" · Live session");
    }
    if app.deck.simulated() {
        title = format!("SIMULATED · {title}");
    }
    let uses_backup_notice = matches!(
        app.deck.shell.nav.current(),
        Route::Backups | Route::BackupForm
    ) || matches!(
        app.deck.shell.nav.current(),
        // Backups' own pages say what their rows just did (#90 moved those rows here).
        Route::GoogleAccount
            | Route::GoogleDisconnectConfirm
            | Route::GoogleDrive
            | Route::BackupDestinations
            | Route::BackupKey
            | Route::BackupQueue
    ) && !app.deck.shell.backup.notice.is_empty();
    let chain_line = app.update_chain_line();
    let busy_line = app
        .deck
        .power_wait_line()
        .or_else(|| app.deck.busy.then(|| app.deck.busy_line()));
    // While a backup job runs on its own pages: what it does and how far it got (#167).
    let backup = &app.deck.shell.backup;
    let backup_line = match (&backup.job, backup.activity) {
        (Some(job), Some(activity)) if uses_backup_notice => Some(job.status_line().replacen(
            "Working:",
            &format!("{activity}:"),
            1,
        )),
        _ => None,
    };
    let note = if app.deck.power_wait.is_some() {
        busy_line.as_deref().unwrap_or_default()
    } else if let Some(line) = &backup_line {
        line.as_str()
    } else if uses_backup_notice {
        app.deck.shell.backup.notice.as_str()
    } else if let Some(line) = &chain_line {
        line.as_str()
    } else if let Some(line) = &busy_line {
        line.as_str()
    } else {
        app.deck.shell.notices.latest().unwrap_or(&app.deck.note)
    };
    let hint = if route == Route::MusicServerForm && app.deck.shell.music_setup.is_some() {
        "Esc back"
    } else if matches!(
        app.deck.shell.nav.current(),
        Route::BackupForm
            | Route::WifiConnect
            | Route::ChangePassword
            | Route::MusicServerForm
            | Route::UpdateBundleForm
            | Route::ManualClock
            | Route::KeyboardWizard
    ) {
        "Type/paste field  Tab/↑↓ choose  Enter next/confirm  Esc cancel"
    } else {
        "↑↓ choose  Enter open  Esc back  F1 help"
    };
    // A box over the page, sized to it, the page still showing around it. Its width comes from
    // what stays put while it is open (title, rows, keys); the grey notice wraps inside it, so a
    // long one arriving later doesn't stretch the box across the screen.
    let widest = rows
        .iter()
        .map(|r| r.chars().count() + 4)
        .chain([title.chars().count() + 4, hint.chars().count() + 2])
        .max()
        .unwrap_or(0);
    let width = settings_box(area, rows.len(), widest).width;
    let notice_rows = wros_shell::notice_lines(note, width.saturating_sub(2)).len();
    let area = settings_box(area, rows.len() + notice_rows - 1, widest);
    app.settings_area.set(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    let (body, offset) = wros_shell::render_menu_at(
        frame,
        area,
        wros_shell::Menu {
            title: &title,
            rows: &rows,
            kinds: &kinds,
            selected: if app.deck.shell.nav.current() == Route::BackupForm {
                app.deck
                    .shell
                    .backup
                    .form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::WifiConnect {
                app.deck
                    .shell
                    .wifi_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::ChangePassword {
                app.deck
                    .shell
                    .password_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::MusicServerForm {
                app.deck
                    .shell
                    .music_server_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::UpdateBundleForm {
                app.deck
                    .shell
                    .update_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::ManualClock {
                app.deck
                    .shell
                    .clock_form
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else if app.deck.shell.nav.current() == Route::KeyboardWizard {
                app.deck
                    .shell
                    .keyboard_wizard
                    .as_ref()
                    .map_or(0, |form| form.selected)
            } else {
                wros_shell::settle(app.deck.shell.nav.selected(), &kinds, rows.len())
            },
            notice: note,
            hint,
        },
        theme.into(),
    );
    let mut hits = app.settings_hits.borrow_mut();
    hits.clear();
    for line in 0..body.height {
        let i = offset + usize::from(line);
        if i < rows.len() {
            hits.push((Rect::new(body.x, body.y + line, body.width, 1), i));
        }
    }
}

/// Where the Settings box goes on `screen`: centred under the menu bar, as wide
/// as its widest line and as tall as its rows, clear of the status line, so the
/// page shows around it. A screen too small for that gets all of it.
pub(crate) fn settings_box(screen: Rect, rows: usize, widest: usize) -> Rect {
    if screen.width < 70 || screen.height < 20 {
        return screen;
    }
    // The menu bar above and the status line below stay in view.
    let room = Rect::new(screen.x, screen.y + 1, screen.width, screen.height - 2);
    let w = to_u16(widest).saturating_add(6).clamp(56, room.width - 4);
    // Rows, the border, the notice and the hint line, and a line of air.
    let h = to_u16(rows).saturating_add(5).clamp(10, room.height);
    Rect::new(
        room.x + (room.width - w) / 2,
        room.y + (room.height - h) / 2,
        w,
        h,
    )
}

/// Draw the QR code and phone instructions. Keep full blocks where they fit
/// and on the Linux console, whose font may lack half blocks.
fn draw_link_offer(
    frame: &mut ratatui::Frame,
    area: Rect,
    offer: &crate::backup_service::LinkOffer,
    linux_console: bool,
) -> bool {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph, Wrap};
    let modules = offer.qr.first().map_or(0, String::len);
    let rows = offer.qr.len();
    let Some(code_width) = area.width.checked_sub(30).map(usize::from) else {
        return false;
    };
    let code_height = usize::from(area.height);
    let (compact, width, height) = if modules <= code_width / 2 && rows <= code_height {
        (false, to_u16(modules * 2), to_u16(rows))
    } else if !linux_console && modules <= code_width && rows.div_ceil(2) <= code_height {
        (true, to_u16(modules), to_u16(rows.div_ceil(2)))
    } else {
        return false;
    };
    frame.render_widget(Clear, area);
    let top = area.y + (area.height - height) / 2;
    let color = |module| {
        if module == b'0' {
            Color::White
        } else {
            Color::Black
        }
    };
    let code: Vec<Line> = offer
        .qr
        .chunks(if compact { 2 } else { 1 })
        .map(|rows| {
            let lower = rows.get(1).map(String::as_bytes);
            Line::from(
                rows[0]
                    .bytes()
                    .enumerate()
                    .map(|(column, module)| {
                        let style = Style::default().fg(color(module));
                        if compact {
                            let lower_module = lower.map_or(b'0', |row| row[column]);
                            Span::styled("▀", style.bg(color(lower_module)))
                        } else {
                            Span::styled("██", style)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(code),
        Rect::new(area.x + 1, top, width, height),
    );
    let text = vec![
        Line::from("Link Google Drive"),
        Line::from(""),
        Line::from("1. Scan this code with your phone's camera."),
        Line::from("2. Sign in to Google. On the notice about an unverified app, tap"),
        Line::from("   Advanced, then Go to WrOS. Tap Allow."),
        Line::from("3. On the WrOS page, tap Finish linking."),
        Line::from(""),
        Line::from("If the laptop does not answer (school or guest Wi-Fi), tap"),
        Line::from("Approve the laptop instead and type this code on Google's page:"),
        Line::from(""),
        Line::from(Span::styled(
            format!("   {}", offer.code),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from("This screen says when Drive is linked. Escape cancels."),
    ];
    let left = area.x + 1 + width + 3;
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        Rect::new(left, top, area.x + area.width - left, height),
    );
    true
}
