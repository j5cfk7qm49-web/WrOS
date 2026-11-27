use super::{Page, session::LIVE_INSTALL_CHOICES, shortcuts::SHORTCUT_ACTIONS};
use crate::app::{App, Overlay, shell};
use crate::theme::Theme;
use grimoire_core::tutorial;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::time::Instant;
use wros_sys::{PowerAction, keyboard::Layout as KeyboardLayout};

/// Cached-only rendering, responsive down to 80x24 (and safely clipped below).
pub(in crate::app) fn draw(f: &mut Frame, app: &App, area: Rect, t: &Theme) {
    let Overlay::Deck(page) = &app.overlay else {
        return;
    };
    if matches!(page, Page::Settings) {
        shell::draw(f, app, area, t);
        return;
    }
    let area = if matches!(page, Page::LiveInstall { .. }) {
        crate::ui::centred(area, 52, 8)
    } else {
        area
    };
    f.render_widget(Clear, area);
    let block = crate::ui::pane_block("WrOS", true, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 4 || inner.width < 10 {
        return;
    }
    let mut rows: Vec<String> = vec![];
    let mut selected = None;
    let title: String;
    let hint: &str;
    match page {
        Page::Settings => unreachable!("shared shell draws Settings"),
        Page::LiveInstall { sel } => {
            title = "Live session".into();
            selected = Some(*sel);
            rows = LIVE_INSTALL_CHOICES.into_iter().map(String::from).collect();
            hint = "↑↓ choose  Enter select  Esc close";
        }
        Page::Workspaces { sel } => {
            title = "Saved workspaces · two independent editors".into();
            selected = Some(*sel);
            rows = app
                .deck
                .state
                .workspaces
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    format!(
                        "{} {} · outline {} · {} columns{}",
                        if i == app.deck.state.selected {
                            "*"
                        } else {
                            " "
                        },
                        w.name,
                        if w.outline { "on" } else { "off" },
                        w.outline_width,
                        if w.focus { " · focus" } else { "" }
                    )
                })
                .collect();
            hint = "↑↓ choose  Enter restore  +/- outline  < > secondary size  \
                    n clone  b starter  p pin set  Esc close";
        }
        Page::WorkspacePins {
            workspace,
            sel,
            ids,
        } => {
            title = format!("Pin set · {}", app.deck.state.workspaces[*workspace].name);
            selected = Some(*sel);
            rows = app
                .pins
                .state()
                .pins
                .iter()
                .map(|p| {
                    format!(
                        "[{}] #{} {} · {:?}",
                        if ids.contains(&p.id) { 'x' } else { ' ' },
                        p.id,
                        p.source.display(),
                        p.life
                    )
                })
                .collect();
            if rows.is_empty() {
                rows.push(
                    "No cards; create references in Pinboard first. A follows all future cards."
                        .into(),
                );
            }
            hint =
                "↑↓ choose  Space toggle  a all  z none  Enter save set  A follow all  Esc cancel";
        }
        Page::System { sel } => {
            title = app
                .deck
                .snapshot
                .as_ref()
                .map(|s| format!("{}{}", if s.simulated { "SIMULATED · " } else { "" }, s.os))
                .unwrap_or_else(|| "System settings · services loaded on demand".into());
            selected = Some(*sel);
            let s = app.deck.snapshot.as_ref();
            rows = vec![
                "Refresh local status".into(),
                "Enable Wi-Fi radio".into(),
                "Disable Wi-Fi radio".into(),
                "Scan Wi-Fi (explicit radio scan)".into(),
                format!(
                    "Keyboard layout · {}",
                    s.and_then(|s| s.keyboard.as_deref()).unwrap_or("unknown")
                ),
                format!(
                    "Brightness -10% · {}",
                    s.and_then(|s| s.brightness_percent)
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "unavailable".into())
                ),
                "Brightness +10%".into(),
                format!(
                    "Low-power UI + request OS power saver · {}",
                    if app.deck.state.low_power {
                        "on"
                    } else {
                        "off"
                    }
                ),
                if app.deck.suspend_available {
                    "Lock and suspend".into()
                } else {
                    wros_sys::SUSPEND_UNAVAILABLE.into()
                },
                "Shutdown (confirmation required)".into(),
                "Restart (confirmation required)".into(),
                "Keyboard shortcuts".into(),
            ];
            hint = "↑↓ choose  Enter request  Esc close · unavailable services never imply success";
        }
        Page::Keyboard { sel } => {
            title = "Console keyboard · 15-second safe preview".into();
            selected = Some(*sel);
            rows = KeyboardLayout::ALL
                .iter()
                .map(|l| l.id().to_string())
                .collect();
            rows.extend([
                "Appliance tty1 only; desktop/SSH layouts belong to their OS.".into(),
                "Custom layouts require administrator-reviewed registration (planned).".into(),
            ]);
            hint = "↑↓ choose  Enter preview  Esc back · no character remapping in WrOS";
        }
        Page::KeyboardWait => {
            title = "Applying a temporary keyboard layout…".into();
            rows.push("An independent systemd timer owns rollback even if WrOS crashes.".into());
            hint = "Esc cancels when the service replies";
        }
        Page::KeyboardTest { text } => {
            title = app
                .deck
                .preview
                .as_ref()
                .map(|(_, l, end)| {
                    format!(
                        "Test {} · {} seconds left",
                        l.id(),
                        end.saturating_duration_since(Instant::now()).as_secs()
                    )
                })
                .unwrap_or_else(|| "Preview expired".into());
            rows.push("Type here, not in your manuscript:".into());
            rows.push(text.clone());
            rows.push("Enter confirms this layout; Escape restores the committed layout.".into());
            hint = "Enter keep  Esc revert · test text is never saved";
        }
        Page::Power { action, yes } => {
            if *action == PowerAction::Suspend && !app.deck.suspend_available {
                title = "Suspend unavailable".into();
                rows.push(wros_sys::SUSPEND_UNAVAILABLE.into());
                hint = "Esc return · no power operation is available";
            } else {
                title = format!("Confirm {action:?}");
                rows.push(
                    "All manuscript buffers must save successfully before a power request.".into(),
                );
                rows.push(
                    if *yes {
                        "  Cancel     [YES, proceed]"
                    } else {
                        "[Cancel]      Yes, proceed"
                    }
                    .into(),
                );
                hint = "←→ select  Enter confirm  Esc cancel";
            }
        }
        Page::Tutorial { sel } => {
            title = format!(
                "Interactive tutorial · {} / {} completed{}",
                app.deck.progress.completed_lessons().count(),
                tutorial::lessons().len(),
                if tutorial::is_demo(&app.project.root) {
                    format!(
                        " · demo project {}",
                        crate::books::title_of_folder(&app.project.root)
                    )
                } else {
                    " · d opens the safe demo project".into()
                }
            );
            selected = Some(*sel);
            rows = tutorial::lessons()
                .iter()
                .map(|l| {
                    format!(
                        "{:02} [{}] {}{}",
                        l.id,
                        if app.deck.progress.completed.contains(&l.id) {
                            "done"
                        } else if app.deck.progress.skipped.contains(&l.id) {
                            "skip"
                        } else {
                            "    "
                        },
                        l.title,
                        if l.event.is_none() { " (planned)" } else { "" }
                    )
                })
                .collect();
            hint = "↑↓ choose  Enter lesson  d demo project  s skip  Esc close";
        }
        Page::Lesson { id } => {
            let l = &tutorial::lessons()[usize::from(*id) - 1];
            title = format!("Lesson {} · {}", id, l.title);
            rows = crate::ui::wrap_words(&l.instructions, inner.width.saturating_sub(4) as usize);
            rows.push(String::new());
            rows.push(
                if l.event.is_some() {
                    "Enter launches the actual feature; progress is event-verified."
                } else {
                    "PLANNED: cannot start or complete this lesson yet."
                }
                .into(),
            );
            hint = "Enter start/replay  h offline help  Esc lessons";
        }
        Page::Shortcuts { sel, capture } => {
            title = if *capture {
                "Press Ctrl+Alt+A-Z or F10-F12 · Esc cancels"
            } else {
                "Custom shortcut aliases · legacy bindings protected"
            }
            .into();
            selected = Some(*sel);
            rows = SHORTCUT_ACTIONS
                .iter()
                .map(|(action, name)| {
                    format!(
                        "{name:<25} {}",
                        app.deck
                            .shortcuts
                            .bindings
                            .iter()
                            .filter(|b| b.action == *action)
                            .map(|b| b.chord.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect();
            hint = "↑↓ action  Enter add/replace alias  Esc back";
        }
    }
    let width = inner.width.saturating_sub(2) as usize;
    let content_height = inner.height.saturating_sub(4) as usize;
    let start = selected.map_or(0, |sel| {
        sel.saturating_sub(content_height.saturating_sub(1))
    });
    let mut lines = vec![Line::from(Span::styled(
        crate::ui::truncate(&title, width),
        Style::default().fg(t.accent),
    ))];
    for (i, row) in rows.iter().enumerate().skip(start).take(content_height) {
        let prefix = if selected == Some(i) { "▸ " } else { "  " };
        let style = Style::default().fg(if selected == Some(i) {
            t.accent
        } else {
            t.text
        });
        lines.push(Line::from(Span::styled(
            crate::ui::truncate(&format!("{prefix}{row}"), width),
            style,
        )));
    }
    while lines.len() < inner.height.saturating_sub(2) as usize {
        lines.push(Line::from(""));
    }
    let busy_line;
    let note = if matches!(page, Page::LiveInstall { .. }) {
        "Save work somewhere persistent before reboot."
    } else if let Some(line) = app.deck.power_wait_line() {
        busy_line = line;
        &busy_line
    } else if app.deck.busy {
        busy_line = app.deck.busy_line();
        &busy_line
    } else {
        &app.deck.note
    };
    lines.push(Line::from(Span::styled(
        crate::ui::truncate(note, width),
        Style::default().fg(t.dim),
    )));
    lines.push(Line::from(Span::styled(
        crate::ui::truncate(hint, width),
        Style::default().fg(t.accent),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}
