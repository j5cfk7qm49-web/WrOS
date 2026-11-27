use super::state::{Input, Page, Productivity};
use crate::app::{App, Overlay};
use crate::theme::Theme;
use crate::ui::{centred, wrap_words};
use chrono::{Datelike, NaiveDate};
use grimoire_core::productivity::Schedule;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Edits",
            Self::Goals => "Goals",
            Self::Calendar => "Calendar",
            Self::Sprints => "Sprints",
            Self::Achievements => "Collection",
            Self::Challenges => "Challenges",
            Self::GoalTypes => "New goal · choose a metric",
            Self::SprintTypes => "New sprint · choose a variant",
            Self::ChallengeFilters => "Future challenge filters · explicit save",
        }
    }
    fn hints(self) -> &'static str {
        match self {
            Self::Overview => {
                "t tracking · m mode · f footnotes · i idle · a milestones · n notices · d challenges"
            }
            Self::Goals => {
                "n new goal · Space enable/disable selected · Delete remove goal metadata"
            }
            Self::Calendar => concat!(
                "←↑↓→ move · [ ] month · Home today · Enter rest day · * today · R rest\n",
                "n November 50k preset · b start · e end · g target",
            ),
            Self::Sprints => {
                "n new · p pause/resume · x stop · b timed break · k skip break (retained)"
            }
            Self::Achievements => {
                "a enable/disable · n quiet notices · no streak penalties or quality scores"
            }
            Self::Challenges => {
                "a accept · c check measured · x self-report · R reroll · f filters · S optional Spark · ←→ date"
            }
            Self::GoalTypes | Self::SprintTypes => {
                "↑↓ choose · Enter set target · Esc back · no manuscript changed"
            }
            Self::ChallengeFilters => {
                "↑↓ row · Space category · ←→ mode/level/time · Enter save · Esc cancel"
            }
        }
    }
}

pub(in crate::app) fn calendar_grid(
    p: &Productivity,
    selected: NaiveDate,
    t: &Theme,
) -> Vec<Line<'static>> {
    calendar_lines(p.config.schedule.as_ref(), &p.stats_date, selected, t)
}

fn calendar_lines(
    schedule: Option<&Schedule>,
    stats_date: &str,
    selected: NaiveDate,
    t: &Theme,
) -> Vec<Line<'static>> {
    let (year, month) = (selected.year(), selected.month());
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(selected);
    let normal = Style::default().fg(t.text);
    let mut lines = vec![
        Line::styled(format!("  {}", first.format("%B %Y")), normal),
        Line::styled(" Mon  Tue  Wed  Thu  Fri  Sat  Sun ", normal),
    ];
    let mut week = vec![Span::raw("     "); first.weekday().num_days_from_monday() as usize];
    for day in (1..=31).map_while(|day| NaiveDate::from_ymd_opt(year, month, day)) {
        let date = day.to_string();
        let (in_schedule, rest) = schedule.map_or((false, false), |s| {
            (
                s.start <= date && date <= s.end,
                s.rest_days.contains(&date),
            )
        });
        let mut style = normal;
        if !in_schedule {
            style = style.add_modifier(Modifier::DIM);
        }
        if day == selected {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let today = if date == stats_date { '*' } else { ' ' };
        let rest = if rest { 'R' } else { ' ' };
        week.push(Span::styled(
            format!(" {:>2}{today}{rest}", day.day()),
            style,
        ));
        if week.len() == 7 {
            lines.push(Line::from(std::mem::take(&mut week)));
        }
    }
    if !week.is_empty() {
        lines.push(Line::from(week));
    }
    lines.push(Line::default());
    lines
}

pub(in crate::app) fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    match &app.overlay {
        Overlay::ProductivityInput { kind, buf, .. } => f.render_widget(
            TargetInput {
                kind: *kind,
                value: buf,
                message: &app.msg,
                theme,
            },
            area,
        ),
        Overlay::Productivity { page, scroll, .. } => {
            let mut fit = app.productivity.render_fit.get();
            f.render_stateful_widget(
                ProductivityPage::from_app(app, *page, *scroll, theme),
                area,
                &mut fit,
            );
            app.productivity.render_fit.set(fit);
        }
        _ => {}
    }
}

struct TargetInput<'a> {
    kind: Input,
    value: &'a str,
    message: &'a str,
    theme: &'a Theme,
}

impl Widget for TargetInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Self {
            kind,
            value,
            message,
            theme: t,
        } = self;
        let area = centred(area, 56, 12);
        Clear.render(area, buf);
        let block = Block::bordered()
            .title(" Productivity · explicit target/date ")
            .border_style(Style::default().fg(t.accent));
        let inside = block.inner(area);
        block.render(area, buf);
        Paragraph::new(vec![
            Line::from(format!("{kind:?}")),
            Line::from(format!("Value: {value}▏")),
            Line::from("Backspace edits · Enter validates/saves · Esc cancels"),
            Line::from("ISO date YYYY-MM-DD; numeric targets only. No original prose edited."),
        ])
        .wrap(Wrap { trim: false })
        .render(inside, buf);
        if inside.height >= 8 {
            Paragraph::new(message)
                .style(Style::default().fg(t.accent))
                .wrap(Wrap { trim: false })
                .render(
                    Rect::new(inside.x, inside.y + inside.height - 2, inside.width, 2),
                    buf,
                );
        }
    }
}

struct ProductivityPage<'a> {
    page: Page,
    scroll: Option<usize>,
    rows: &'a [String],
    row_selection: Option<usize>,
    schedule: Option<&'a Schedule>,
    calendar_date: NaiveDate,
    stats_date: &'a str,
    message: &'a str,
    theme: &'a Theme,
}

impl<'a> ProductivityPage<'a> {
    fn from_app(app: &'a App, page: Page, scroll: Option<usize>, theme: &'a Theme) -> Self {
        let p = &app.productivity;
        Self {
            page,
            scroll,
            rows: &p.rows,
            row_selection: p.row_selection,
            schedule: p.config.schedule.as_ref(),
            calendar_date: p.calendar_date,
            stats_date: &p.stats_date,
            message: &app.msg,
            theme,
        }
    }
}

impl StatefulWidget for ProductivityPage<'_> {
    type State = (usize, usize);

    fn render(self, area: Rect, buf: &mut Buffer, fit: &mut Self::State) {
        let Self {
            page,
            scroll,
            rows,
            row_selection,
            schedule,
            calendar_date,
            stats_date,
            message,
            theme: t,
        } = self;
        let area = centred(area, 76, 22);
        Clear.render(area, buf);
        let block = Block::bordered()
            .title(format!(" {} ", page.title()))
            .border_style(Style::default().fg(t.accent));
        let inside = block.inner(area);
        block.render(area, buf);
        if inside.height < 5 || inside.width < 8 {
            return;
        }
        let footer_h = 5.min(inside.height.saturating_sub(2));
        let body = Rect::new(inside.x, inside.y, inside.width, inside.height - footer_h);
        let footer = Rect::new(inside.x, body.y + body.height, inside.width, footer_h);
        let mut lines = if page == Page::Calendar {
            calendar_lines(schedule, stats_date, calendar_date, t)
        } else {
            Vec::new()
        };
        let mut selected_at = None;
        for (i, row) in rows.iter().enumerate() {
            let chosen = row_selection == Some(i);
            if chosen {
                selected_at = Some(lines.len());
            }
            for text in wrap_words(row, body.width.saturating_sub(2) as usize) {
                lines.push(Line::styled(
                    format!("{}{text}", if chosen { "› " } else { "  " }),
                    Style::default().fg(if chosen { t.accent } else { t.text }),
                ));
            }
        }
        let max = lines.len().saturating_sub(body.height as usize);
        let offset = scroll
            .unwrap_or_else(|| {
                selected_at
                    .unwrap_or(0)
                    .saturating_sub(body.height as usize / 2)
            })
            .min(max);
        *fit = (offset, max);
        Paragraph::new(
            lines
                .into_iter()
                .skip(offset)
                .take(body.height as usize)
                .collect::<Vec<_>>(),
        )
        .render(body, buf);
        Paragraph::new(format!(
            "{}\nr reload/retry · PgUp/PgDn scroll · Esc back · F1 policy",
            page.hints()
        ))
        .style(Style::default().fg(t.dim))
        .wrap(Wrap { trim: false })
        .render(
            Rect::new(
                footer.x,
                footer.y,
                footer.width,
                footer.height.saturating_sub(2),
            ),
            buf,
        );
        Paragraph::new(message)
            .style(Style::default().fg(t.accent))
            .wrap(Wrap { trim: false })
            .render(
                Rect::new(
                    footer.x,
                    footer.y + footer.height.saturating_sub(2),
                    footer.width,
                    2,
                ),
                buf,
            );
    }
}
