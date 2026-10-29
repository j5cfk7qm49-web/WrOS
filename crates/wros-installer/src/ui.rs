//! Forms use the same palette, panel geometry and selection as native Settings.
use crate::{Kind, Session};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::{Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use wros_shell::grimoire_theme;

pub fn render(
    frame: &mut Frame,
    session: &Session,
    theme: &grimoire_theme::Theme,
    scroll: u16,
    help: bool,
) {
    let position = session.position();
    let status = if session.showing {
        format!("{} · {} of {}", session.title, position.0, position.1)
    } else {
        session.title.clone()
    };
    let hint = if help {
        "PgUp/PgDn read  F1 / Esc close"
    } else if session
        .active()
        .is_some_and(|q| q.kind == Kind::Multiselect)
    {
        "↑↓ choose  Space mark  Enter continue  Esc back  F1 help"
    } else if session.showing {
        "↑↓ choose  Enter continue  Esc back  F1 help"
    } else {
        "F1 help"
    };
    let body = wros_shell::render_panel(
        frame,
        frame.area(),
        "WrOS · Install",
        &status,
        hint,
        theme.into(),
    );
    if help {
        frame.render_widget(Paragraph::new(
            "Installation help\n\n↑/↓ choose an option. Space marks a checklist item.\nEnter accepts the current field and continues.\nEscape returns to an earlier question when available.\nPage Up/Down scroll the explanation. F1 closes help.\n\nPasswords stay hidden. Disk-write questions begin at No.\nDebian Installer performs partitioning, encryption, account setup and installation."
        ).style(Style::default().fg(theme.text)).wrap(Wrap{trim:false}).scroll((scroll,0)),body);
        return;
    }
    if let Some(question) = session.active() {
        let mut lines = question
            .description
            .lines()
            .map(|line| Line::styled(line, Style::default().fg(theme.accent)))
            .collect::<Vec<_>>();
        lines.push(Line::raw(""));
        lines.extend(question.detail.lines().map(Line::raw));
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        // Ask the existing paragraph widget for its wrapped height. Short
        // prompts stay together; long explanations retain a scrollable area.
        let height = paragraph
            .line_count(body.width)
            .saturating_add(1)
            .min((body.height / 2).max(1) as usize) as u16;
        let [description, input] =
            Layout::vertical([Constraint::Length(height), Constraint::Min(1)]).areas(body);
        frame.render_widget(paragraph, description);
        if matches!(
            question.kind,
            Kind::Select | Kind::Multiselect | Kind::Boolean
        ) {
            let rows = if question.kind == Kind::Boolean {
                vec!["No".into(), "Yes".into()]
            } else {
                question
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if question.kind == Kind::Multiselect {
                            format!(
                                "[{}] {c}",
                                if question.checked.get(i) == Some(&true) {
                                    'x'
                                } else {
                                    ' '
                                }
                            )
                        } else {
                            c.clone()
                        }
                    })
                    .collect::<Vec<String>>()
            };
            frame.render_stateful_widget(
                List::new(rows.into_iter().map(ListItem::new))
                    .style(Style::default().fg(theme.text))
                    .highlight_style(Style::default().fg(theme.accent))
                    .highlight_symbol("▸ "),
                input,
                &mut ListState::default().with_selected(Some(question.selected)),
            );
        } else {
            let shown = match question.kind {
                Kind::Note | Kind::Text | Kind::Error => "Enter to continue",
                _ => question.shown_answer(),
            };
            // Show the editable tail at narrow widths; passwords have a fixed
            // placeholder and never enter a terminal buffer, even briefly.
            let count = input.width.saturating_sub(2) as usize;
            let tail = shown
                .chars()
                .rev()
                .take(count)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            frame.render_widget(
                Paragraph::new(format!("▸ {tail}")).style(Style::default().fg(theme.accent)),
                input,
            );
        }
    } else if let Some(progress) = &session.progress {
        let [text, gauge] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(body);
        frame.render_widget(
            Paragraph::new(format!("{}\n\n{}", progress.title, progress.detail))
                .style(Style::default().fg(theme.text))
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            text,
        );
        let ratio = (progress.value - progress.min) as f64 / (progress.max - progress.min) as f64;
        frame.render_widget(
            Gauge::default()
                .ratio(ratio.clamp(0., 1.))
                .gauge_style(Style::default().fg(theme.accent)),
            gauge,
        );
    } else {
        frame.render_widget(
            Paragraph::new("Preparing installation…").style(Style::default().fg(theme.dim)),
            body,
        );
    }
}
