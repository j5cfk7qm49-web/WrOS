#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use super::{App, Theme, fit_text, paint_band};
use crate::geometry::to_u16;
use crate::motion::Marquee;
use crate::ui::thousands;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{StatefulWidget, Widget};
use std::time::Instant;

pub(crate) struct ProgressBar<'a> {
    total: usize,
    target: usize,
    count: Span<'static>,
    goal: Span<'static>,
    daily: Span<'static>,
    sprint: Option<Span<'static>>,
    message: Option<Span<'static>>,
    track: Option<String>,
    console: bool,
    low_power: bool,
    now: Instant,
    theme: &'a Theme,
}

pub(crate) struct ProgressState<'a> {
    pub(crate) hit: &'a mut Rect,
    pub(crate) marquee: &'a mut Option<Marquee>,
}

impl<'a> ProgressBar<'a> {
    pub(crate) fn from_app(app: &App, t: &'a Theme, now: Instant) -> Self {
        let total = app.project.total_words();
        let target = app.project.meta.target_words.max(1);
        let dim = Style::default().fg(t.dim);
        let accent = Style::default().fg(t.accent);
        let count = Span::styled(
            format!(" {}", thousands(total)),
            Style::default().fg(t.text),
        );
        let goal = Span::styled(format!(" / {} ", thousands(target)), dim);
        let daily = daily_words(app, t);
        let sprint = app.sprint_label().map(|(label, reached)| {
            let style = if reached {
                accent
            } else {
                Style::default().fg(t.text)
            };
            Span::styled(format!("  {label}"), style)
        });

        let message = if let Some(battery) = app.deck.battery.showing(now) {
            Some(Span::styled(
                battery.to_owned(),
                Style::default().fg(t.warn),
            ))
        } else if let Some((says, _)) = app.bar_description() {
            Some(Span::styled(says, dim.add_modifier(Modifier::ITALIC)))
        } else {
            (!app.msg.is_empty()).then(|| Span::styled(app.msg.clone(), accent))
        };
        let track = match &app.music.state {
            crate::music::State::Playing(track) if track.playing && message.is_none() => {
                Some(track.title.trim())
                    .filter(|title| !title.is_empty())
                    .map(str::to_owned)
            }
            _ => None,
        };

        Self {
            total,
            target,
            count,
            goal,
            daily,
            sprint,
            message,
            track,
            console: app.linux_console,
            low_power: app.deck.state.low_power,
            now,
            theme: t,
        }
    }

    fn render_line(self, area: Rect, buf: &mut Buffer, state: &mut ProgressState<'_>) {
        if area.is_empty() {
            *state.marquee = None;
            return;
        }
        let Self {
            total,
            target,
            count,
            goal,
            daily,
            sprint,
            mut message,
            track,
            console,
            low_power,
            now,
            theme: t,
        } = self;
        let dim = Style::default().fg(t.dim);
        let accent = Style::default().fg(t.accent);
        let width = usize::from(area.width);
        // A message is something to read now, so it wins room: the bar shrinks away first, then the
        // total and target give way (today's count stays), and only a message too long even for that
        // takes the whole line, until it clears.
        if let Some(shown) = &message {
            let today = daily.width() + sprint.as_ref().map_or(0, Span::width);
            let beside_all = width.saturating_sub(count.width() + goal.width() + today + 3);
            let beside_today = width.saturating_sub(today + 3);
            if shown.width() > beside_all {
                *state.marquee = None;
                *state.hit = Rect::default();
                if shown.width() <= beside_today {
                    let mut left = vec![daily.clone()];
                    left.extend(sprint.clone());
                    Line::from(left).render(area, buf);
                    let right = Rect::new(
                        area.right() - 1 - to_u16(shown.width()),
                        area.y,
                        to_u16(shown.width()),
                        1,
                    );
                    Line::styled(shown.content.clone(), shown.style).render(right, buf);
                } else {
                    let text = fit_text(&shown.content, width.saturating_sub(2), console);
                    Line::styled(format!(" {text}"), shown.style).render(area, buf);
                }
                return;
            }
        }
        let counts_width = count.width() + goal.width();
        let room = width
            .saturating_sub(counts_width + daily.width() + sprint.as_ref().map_or(0, Span::width));
        let music_prefix = if console { "Music " } else { "♪ " };
        let prefix_width = Span::raw(music_prefix).width();
        let right_width = if let Some(message) = &message {
            message.width()
        } else {
            track.as_deref().map_or(0, |title| {
                let music_room = room.saturating_sub(width / 2 + 3).max(12);
                (prefix_width + Span::raw(title).width()).min(music_room)
            })
        };
        let min_bar = if message.is_some() {
            0
        } else {
            (width / 2).min(10).min(room)
        };
        let right_room = room.saturating_sub(min_bar + 3).min(right_width);
        let bar_width = room
            .saturating_sub(if right_room == 0 { 0 } else { right_room + 3 })
            .min(width / 2);
        let filled =
            usize::try_from(total.min(target) as u128 * bar_width as u128 / target as u128)
                .unwrap_or(bar_width);
        let (written, remaining) = if console { ("=", "-") } else { ("━", "─") };
        *state.hit = Rect::new(
            area.x,
            area.y,
            to_u16((counts_width + bar_width).min(width)),
            1,
        );
        let mut progress = vec![
            count,
            goal,
            Span::styled(written.repeat(filled), accent),
            Span::styled(remaining.repeat(bar_width - filled), dim),
            daily,
        ];
        progress.extend(sprint);
        Line::from(progress).render(area, buf);

        let title_room = right_room.saturating_sub(prefix_width);
        if let Some(title) = track.as_deref().filter(|_| title_room > 0) {
            let shown = if !low_power && Span::raw(title).width() > title_room {
                if state
                    .marquee
                    .as_ref()
                    .is_some_and(|m| !m.matches(title, title_room))
                {
                    *state.marquee = None;
                }
                let marquee = state
                    .marquee
                    .get_or_insert_with(|| Marquee::new(title, title_room, now));
                marquee.advance(now);
                marquee.visible().to_owned()
            } else {
                *state.marquee = None;
                fit_text(title, title_room, console)
            };
            message = Some(Span::styled(format!("{music_prefix}{shown}"), dim));
        } else {
            *state.marquee = None;
        }
        if let Some(message) = message.filter(|_| right_room > 0) {
            let text = fit_text(&message.content, right_room, console);
            let right = Rect::new(
                area.right() - 1 - to_u16(right_room),
                area.y,
                to_u16(right_room),
                1,
            );
            Line::styled(text, message.style).render(right, buf);
        }
    }
}

impl<'a> StatefulWidget for ProgressBar<'a> {
    type State = ProgressState<'a>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let (theme, console) = (self.theme, self.console);
        self.render_line(area, buf, state);
        paint_band(buf, area, theme, console);
    }
}

// Keep the existing daily count formatting and target comparison on every target.
#[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn daily_words(app: &App, t: &Theme) -> Span<'static> {
    let dim = Style::default().fg(t.dim);
    if app.today_is_rest_day() {
        return Span::styled("  rest day", dim);
    }
    let today = app.today_words();
    let sign = if today >= 0 {
        ""
    } else if app.linux_console {
        "-"
    } else {
        "−"
    };
    let words = thousands(today.unsigned_abs() as usize);
    let style = if today >= app.project.meta.daily_target as i64 {
        Style::default().fg(t.accent)
    } else {
        dim
    };
    Span::styled(format!("  today {sign}{words}"), style)
}
