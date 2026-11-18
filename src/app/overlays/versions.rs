//! A scene's kept versions, with what changed since each.

use super::*;

impl App {
    pub(super) fn on_history_key(&mut self, key: Key) {
        let Overlay::History {
            scene,
            versions,
            sel,
            scroll,
            ..
        } = &mut self.overlay
        else {
            return;
        };
        // PgUp/PgDn scroll the changes, so only ↑↓ and j/k move the list.
        let mut next = *sel;
        if list_nav(key, &mut next, versions.len(), LINES) {
            while matches!(versions.get(next), Some(HistoryItem::Session(_))) {
                let before = next;
                list_nav(key, &mut next, versions.len(), LINES);
                if next == before {
                    return;
                }
            }
            *sel = next;
            *scroll = 0;
            return;
        }
        match key {
            Key::PageDown | Key::Char(' ') if !versions.is_empty() => *scroll += 10,
            Key::PageUp => *scroll = scroll.saturating_sub(10),
            Key::Enter => {
                if let Some(HistoryItem::Version { text, .. }) = versions.get(*sel) {
                    let (scene, text) = (scene.clone(), text.clone());
                    self.overlay = Overlay::None;
                    self.restore_version(&scene, &text);
                }
            }
            Key::Esc => self.overlay = Overlay::None,
            _ => {}
        }
    }
}

pub(super) fn draw_history(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::History {
        scene,
        title,
        versions,
        timeline,
        sel,
        scroll,
    } = &app.overlay
    else {
        return;
    };
    let box_area = centred(
        area,
        area.width.saturating_sub(4).min(118),
        area.height.saturating_sub(2).min(40),
    );
    f.render_widget(Clear, box_area);
    let count = versions
        .iter()
        .filter(|item| matches!(item, HistoryItem::Version { .. }))
        .count();
    let head = format!(
        "HISTORY · {} · {count} version{}",
        title.to_uppercase(),
        if count == 1 { "" } else { "s" }
    );
    let block = pane_block(&head, true, t);
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    let dim = Style::default().fg(t.dim);
    let notice = match timeline {
        HistoryTimeline::Reading(_) => Some("Reading the timeline…"),
        HistoryTimeline::Unavailable => Some("Timeline unavailable; showing saved copies."),
        _ => None,
    };
    let inner = if let Some(notice) = notice {
        let [status, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
        f.render_widget(
            Paragraph::new(Line::styled(format!(" {notice}"), dim)),
            status,
        );
        rest
    } else {
        inner
    };
    let list_width = if versions.iter().any(|item| {
        matches!(
            item,
            HistoryItem::Version {
                from_timeline: true,
                ..
            }
        )
    }) {
        inner.width.saturating_sub(32).clamp(36, 52)
    } else {
        36
    };
    let [list_area, _, diff_area] = Layout::horizontal([
        Constraint::Length(list_width),
        Constraint::Length(2),
        Constraint::Min(20),
    ])
    .areas(inner);
    let Some(HistoryItem::Version {
        when, label, text, ..
    }) = versions.get(*sel)
    else {
        let mut lines = Vec::new();
        if !matches!(timeline, HistoryTimeline::Reading(_)) {
            lines.push(Line::styled(" No versions yet.", dim));
        }
        lines.push(hint_line("esc close", t));
        f.render_widget(Paragraph::new(lines), inner);
        return;
    };
    let current = app
        .project
        .nodes
        .iter()
        .find(|n| &n.path == scene)
        .map(|n| n.body.clone())
        .unwrap_or_default();
    let now_words = grimoire_core::notes::count_words(&current) as i64;

    // The versions, newest first.
    let room = list_area.height.saturating_sub(3) as usize;
    let start = sel.saturating_sub(room.saturating_sub(1));
    let mut left: Vec<Line> = vec![
        Line::from(Span::styled(" kept versions · length vs now", dim)),
        Line::from(""),
    ];
    for (i, item) in versions.iter().enumerate().skip(start).take(room) {
        let (when, label, text, from_timeline) = match item {
            HistoryItem::Session(label) => {
                left.push(Line::styled(
                    truncate(&format!(" ─ {label}"), list_area.width as usize),
                    dim,
                ));
                continue;
            }
            HistoryItem::Version {
                when,
                label,
                text,
                from_timeline,
            } => (when, label, text, *from_timeline),
        };
        let on = i == *sel;
        let (time, detail) = if from_timeline {
            let time = format!("{}  ", when.format("%d %b %H:%M"));
            let room = (list_area.width as usize).saturating_sub(3 + time.len());
            (time, truncate(label.as_deref().unwrap_or_default(), room))
        } else {
            let body = project::split_frontmatter(text).1;
            let words = grimoire_core::notes::count_words(&body) as i64;
            (
                format!("{:<17}", when_label(*when)),
                truncate(&length_vs_now(words - now_words), 16),
            )
        };
        let row = Line::from(vec![
            Span::styled(
                if on { " ▸ " } else { "   " },
                Style::default().fg(t.accent),
            ),
            Span::styled(
                time,
                Style::default().fg(if on { t.accent } else { t.text }),
            ),
            Span::styled(detail, dim),
        ]);
        left.push(if on {
            row.style(Style::default().bg(t.sel))
        } else {
            row
        });
    }
    f.render_widget(Paragraph::new(left), list_area);

    // What changed between that version and now.
    let old = project::split_frontmatter(text).1;
    let words = grimoire_core::notes::count_words(&old);
    let pieces = grimoire_core::history::diff(&old, &current);
    let width = diff_area.width.saturating_sub(1) as usize;
    let (mut body, first_change) = diff_lines(&pieces, width, t);
    let mut header = vec![Line::from(vec![
        Span::styled(
            format!("{} · {} words", when_label(*when), thousands(words)),
            Style::default().fg(t.accent),
        ),
        Span::styled(
            format!("  ·  now {} words", thousands(now_words as usize)),
            dim,
        ),
    ])];
    if let Some(label) = label {
        header.extend(
            wrap_words(label, width)
                .into_iter()
                .map(|row| Line::styled(row, dim)),
        );
    }
    header.extend([
        Line::from(vec![
            Span::styled(
                "struck",
                Style::default()
                    .fg(t.warn)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            Span::styled(" was in this version and is gone now · ", dim),
            Span::styled(
                "underlined",
                Style::default()
                    .fg(t.accent)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(" is new since", dim),
        ]),
        Line::from(""),
    ]);
    let room = (diff_area.height as usize).saturating_sub(header.len() + 2);
    let top = first_change.saturating_sub(2) + *scroll;
    let top = top.min(body.len().saturating_sub(1));
    let mut lines = header;
    if pieces
        .iter()
        .all(|p| matches!(p, grimoire_core::history::Piece::Same(_)))
    {
        lines.push(Line::from(Span::styled(
            "identical to the scene as it is now",
            dim,
        )));
    } else {
        lines.extend(body.drain(..).skip(top).take(room));
    }
    while lines.len() < (diff_area.height as usize).saturating_sub(1) {
        lines.push(Line::from(""));
    }
    lines.push(hint_line(
        "↑↓ pick a version   PgUp PgDn scroll   ↵ restore it   esc close",
        t,
    ));
    f.render_widget(Paragraph::new(lines), diff_area);
}

/// A kept version's length against the scene now, in 16 columns or fewer:
/// "6 words shorter", "1,204 words longer", "12,345 shorter", "same length".
pub(super) fn length_vs_now(delta: i64) -> String {
    let n = thousands(delta.unsigned_abs() as usize);
    let (way, one) = match delta {
        0 => return "same length".into(),
        d if d < 0 => ("shorter", "word"),
        _ => ("longer", "word"),
    };
    let plural = if delta.unsigned_abs() == 1 {
        one.to_string()
    } else {
        format!("{one}s")
    };
    let long = format!("{n} {plural} {way}");
    if long.chars().count() <= 16 {
        long
    } else {
        format!("{n} {way}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_says_how_its_length_compares_with_now() {
        assert_eq!(length_vs_now(0), "same length");
        assert_eq!(length_vs_now(-6), "6 words shorter");
        assert_eq!(length_vs_now(1), "1 word longer");
        assert_eq!(length_vs_now(12), "12 words longer");
        assert_eq!(length_vs_now(-12_345), "12,345 shorter");
        for d in [-99_999i64, -999, 7, 99_999] {
            assert!(length_vs_now(d).chars().count() <= 16, "{d}");
        }
    }
}
