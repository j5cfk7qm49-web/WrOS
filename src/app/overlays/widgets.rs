//! The quick controls above the info bar's Wi-Fi, sound, battery and clock gauges.

use super::{TYPED, bar, list_nav};
use crate::app::shell::{Panel, Route, WIFI_SCAN_FAILED, path_to};
use crate::app::{App, Key, Overlay, hit, productivity};
use crate::gauges::{Widget, Wifi};
use crate::music::State;
use crate::theme::Theme;
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use wros_sys::{Request, WifiNetwork};

enum Choice {
    Network(WifiNetwork),
    Wifi(bool),
    Volume(u8),
    Mute(bool),
    Brightness(u8),
    PlayPause,
    NextTrack,
    Settings(Route),
}

struct Row {
    line: Line<'static>,
    choice: Option<Choice>,
}

impl Row {
    fn info(line: impl Into<Line<'static>>) -> Self {
        Self {
            line: line.into(),
            choice: None,
        }
    }

    fn action(line: impl Into<Line<'static>>, choice: Choice) -> Self {
        Self {
            line: line.into(),
            choice: Some(choice),
        }
    }
}

fn rows(app: &App, kind: Widget, t: &Theme) -> Vec<Row> {
    let gauges = app.deck.gauges;
    let console = app.linux_console;
    let more = if console { "..." } else { "…" };
    match kind {
        Widget::Wifi => {
            let networks = app.deck.shell.wifi_networks.as_deref();
            let on = gauges.wifi_radio.unwrap_or_else(|| {
                matches!(gauges.wifi, Some(Wifi::Bars(_) | Wifi::Connected))
                    || networks.is_some_and(|networks| !networks.is_empty())
            });
            let mut rows: Vec<_> = networks
                .unwrap_or_default()
                .iter()
                .map(|network| {
                    let name = &network.ssid;
                    let signal = network.signal;
                    if network.in_use {
                        let check = if console { "[x]" } else { "✓" };
                        Row::info(format!("{check} {name}  {signal}%"))
                    } else {
                        Row::action(
                            format!("{name}  {signal}%"),
                            Choice::Network(network.clone()),
                        )
                    }
                })
                .collect();
            if rows.is_empty() {
                rows.push(Row::info(if networks.is_none() && app.deck.busy {
                    format!("Looking for networks{more}")
                } else if !on && gauges.wifi_radio == Some(false) {
                    "Wi-Fi is off".into()
                } else if networks.is_some() {
                    "No visible networks".into()
                } else if app.deck.note.contains(WIFI_SCAN_FAILED) {
                    WIFI_SCAN_FAILED.into()
                } else {
                    format!("Looking for networks{more}")
                }));
            }
            rows.extend([
                Row::action(
                    if on {
                        "Turn Wi-Fi off"
                    } else {
                        "Turn Wi-Fi on"
                    },
                    Choice::Wifi(!on),
                ),
                Row::action(
                    format!("Network settings{more}"),
                    Choice::Settings(Route::Panel(Panel::Network)),
                ),
            ]);
            rows
        }
        Widget::Sound => {
            let volume = gauges.volume;
            let available = volume.is_some() || gauges.muted;
            let mut rows = vec![
                slider("Volume", volume, Choice::Volume, console, t),
                if available {
                    Row::action(
                        if gauges.muted { "Unmute" } else { "Mute" },
                        Choice::Mute(!gauges.muted),
                    )
                } else {
                    Row::info("Mute unavailable")
                },
                Row::info(if available {
                    "Output: Default sound card"
                } else {
                    "Output unavailable"
                }),
            ];
            let (track, play_pause) = match &app.music.state {
                State::Playing(track) => {
                    let title = if track.artist.is_empty() {
                        track.title.clone()
                    } else {
                        format!("{} / {}", track.title, track.artist)
                    };
                    (title, if track.playing { "Pause" } else { "Play" })
                }
                _ => ("No track loaded".into(), "Play / pause"),
            };
            rows.extend([
                Row::info(track),
                Row::action(play_pause, Choice::PlayPause),
                Row::action("Next track", Choice::NextTrack),
                Row::action(
                    format!("Sound settings{more}"),
                    Choice::Settings(Route::SoundView),
                ),
            ]);
            rows
        }
        Widget::Battery => {
            let charge = gauges.battery.map_or_else(
                || "Battery charge unavailable".into(),
                |percent| {
                    if gauges.charging {
                        format!("Battery {percent}% (charging)")
                    } else if let Some(minutes) = gauges.battery_minutes {
                        let (hours, minutes) = (minutes / 60, minutes % 60);
                        format!("Battery {percent}%  /  {hours}h {minutes:02}m left")
                    } else {
                        format!("Battery {percent}%  /  time left unavailable")
                    }
                },
            );
            vec![
                Row::info(charge),
                slider("Brightness", gauges.light, Choice::Brightness, console, t),
                if app.warm_screen_available() {
                    Row::action(
                        format!("Night light{more}"),
                        Choice::Settings(Route::WarmScreen),
                    )
                } else {
                    Row::info("Night light unavailable")
                },
                Row::action(
                    format!("Battery settings{more}"),
                    Choice::Settings(Route::Panel(Panel::Power)),
                ),
            ]
        }
        Widget::Clock => {
            let date = NaiveDate::parse_from_str(&app.productivity.stats_date, "%Y-%m-%d")
                .unwrap_or_else(|_| app.productivity.clock.now().wall.date_naive());
            let mut rows = vec![Row::info(date.format("%A, %B %-d, %Y").to_string())];
            rows.extend(
                productivity::calendar_grid(&app.productivity, date, t)
                    .into_iter()
                    .map(Row::info),
            );
            rows.push(Row::action(
                format!("Date and time settings{more}"),
                Choice::Settings(Route::Panel(Panel::DateTime)),
            ));
            rows
        }
    }
}

fn slider(
    label: &str,
    value: Option<u8>,
    choice: impl FnOnce(u8) -> Choice,
    console: bool,
    t: &Theme,
) -> Row {
    let Some(value) = value else {
        return Row::info(format!("{label} unavailable"));
    };
    let filled = usize::from(value.min(100) / 5);
    let (full, empty) = if console { ("#", "-") } else { ("━", "─") };
    let line = Line::from(vec![
        Span::raw(format!("{label} {value:>3}%  ")),
        Span::styled(full.repeat(filled), Style::default().fg(t.accent)),
        Span::styled(empty.repeat(20 - filled), Style::default().fg(t.dim)),
    ]);
    Row::action(line, choice(value))
}

impl App {
    pub(in crate::app) fn open_widget(&mut self, kind: Widget) {
        let sel = if kind == Widget::Battery { 1 } else { 0 };
        self.widget_panel.set((Rect::default(), 0));
        self.overlay = Overlay::Widget { kind, sel };
        if kind == Widget::Wifi {
            self.scan_widget_wifi();
        }
    }

    pub(super) fn on_widget_key(&mut self, kind: Widget, mut sel: usize, key: Key) {
        if key == Key::Esc {
            self.overlay = Overlay::None;
            return;
        }
        if matches!(key, Key::Char(_)) {
            return;
        }
        let rows = rows(self, kind, &self.theme);
        sel = sel.min(rows.len() - 1);
        if list_nav(key, &mut sel, rows.len(), TYPED) {
            self.overlay = Overlay::Widget { kind, sel };
        } else if let Some(choice) = rows.into_iter().nth(sel).and_then(|row| row.choice) {
            self.choose_widget_row(choice, key);
        }
    }

    pub(in crate::app) fn widget_click(&mut self, x: u16, y: u16) -> bool {
        let Overlay::Widget { kind, .. } = self.overlay else {
            return false;
        };
        let (panel, first) = self.widget_panel.get();
        if hit(panel, x, y) {
            let sel = first + usize::from(y - panel.y);
            self.overlay = Overlay::Widget { kind, sel };
            self.on_widget_key(kind, sel, Key::Enter);
        } else {
            self.overlay = Overlay::None;
        }
        true
    }

    fn choose_widget_row(&mut self, choice: Choice, key: Key) {
        let adjustment = match key {
            Key::Left => -5,
            Key::Right => 5,
            Key::Enter => 0,
            _ => return,
        };
        let request = match (choice, key) {
            (Choice::Network(network), Key::Enter) if !self.deck.busy => {
                let mut path = path_to(Route::Panel(Panel::Network));
                path.push(Route::WifiNetworks);
                self.open_settings_at(&path);
                self.open_wifi_network(network);
                return;
            }
            (Choice::Wifi(on), Key::Enter) => Request::Wifi(on),
            (Choice::Mute(muted), Key::Enter) => Request::SetMute(muted),
            (Choice::Volume(current), Key::Left | Key::Right) => {
                Request::SetVolume((i16::from(current) + adjustment).clamp(0, 100) as u8)
            }
            (Choice::Brightness(current), Key::Left | Key::Right) => {
                Request::Brightness((i16::from(current) + adjustment).clamp(5, 100) as u8)
            }
            (Choice::PlayPause, Key::Enter) => return self.on_function_key(5),
            (Choice::NextTrack, Key::Enter) => return self.on_function_key(6),
            (Choice::Settings(route), Key::Enter) => {
                if route == Route::WarmScreen {
                    let mut path = path_to(Route::Panel(Panel::Display));
                    path.push(route);
                    self.open_settings_at(&path);
                } else {
                    self.open_settings_page(route);
                }
                if route == Route::SoundView && !self.deck.busy {
                    self.deck.request(Request::AudioStatus);
                }
                return;
            }
            _ => return,
        };
        if !self.deck.busy {
            self.deck.request(request);
        }
    }
}

pub(super) fn draw(
    f: &mut Frame,
    app: &App,
    area: Rect,
    info: Rect,
    kind: Widget,
    sel: usize,
    t: &Theme,
) {
    let anchor = app
        .widget_hits()
        .iter()
        .find(|(_, widget)| *widget == kind)
        .map_or(info, |&(rect, _)| rect);
    let rows = rows(app, kind, t);
    let sel = sel.min(rows.len() - 1);
    let right = anchor.right().min(area.right());
    let width = rows.iter().map(|row| row.line.width()).max().unwrap_or(0) + 5;
    let width = width.min(usize::from(right.saturating_sub(area.x))) as u16;
    let room = usize::from(anchor.y.saturating_sub(area.y));
    let shown = rows.len().min(bar::MAX_ROWS).min(room);
    let start = (sel + 1).saturating_sub(shown);
    let panel = Rect::new(right - width, anchor.y - shown as u16, width, shown as u16);
    app.widget_panel.set((panel, start));
    let (inner, band) = bar::draw_panel(f, area, panel, app.linux_console, t);
    for (offset, row) in rows.into_iter().skip(start).take(shown).enumerate() {
        let at = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
        let mut line = row.line;
        let marker = if start + offset == sel {
            let style = bar::light_panel_row(f, panel, at.y, app.linux_console, t);
            line.style = style;
            for span in &mut line.spans {
                span.style = span.style.patch(style);
            }
            if app.linux_console {
                "> "
            } else {
                wros_shell::look::MARKER
            }
        } else {
            line.style = Style::default().fg(t.text).bg(band);
            "  "
        };
        line.spans.insert(0, Span::raw(marker));
        f.render_widget(Paragraph::new(line), at);
    }
}
