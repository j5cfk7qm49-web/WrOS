//! Rendering. Panes, bottom bars, and the theme picker overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};
use std::time::{Instant, SystemTime};

use crate::app::outline::DeskView;
use crate::app::{App, Focus, Overlay};
use crate::gauges::{self, Item};
use crate::motion::Marquee;
use crate::scene::{self, Ink, Mode, Phase};
use crate::theme::Theme;
use grimoire_core::create::{self, New};
use grimoire_core::documents::Pane;
use grimoire_core::project::{Area, Kind};

/// Wide enough that the scene's 28 columns fit inside the border.
pub const LEFT_W: u16 = (scene::W + 2) as u16;

const SCENE_H: u16 = (scene::H + 2) as u16;
const MIN_TREE: u16 = 6;
const MIN_LEFT: u16 = 18;
const MIN_EDIT: u16 = 24;

// The native renderer's one-cell tiles. Keep the same code points available to
// TestBackend in builds without wros-graphics.
pub(crate) const TILE_LEFT: char = '\u{E100}';
pub(crate) const TILE_RIGHT: char = '\u{E101}';
pub(crate) const TILE_RIGHT_DUE: char = '\u{E102}';
pub(crate) const TILE_LETTERS: [char; 4] = ['\u{E110}', '\u{E111}', '\u{E112}', '\u{E113}'];
pub(crate) const TILE_POWER: char = '\u{E114}';

pub fn draw(
    f: &mut Frame,
    app: &mut App,
    motion: Option<(&mut crate::motion::Motion, bool)>,
) -> bool {
    // Cloned once per frame so sub-renderers can read the theme while `app`
    // stays mutably borrowed for scroll and rect bookkeeping.
    let mut t = if app.linux_console {
        app.theme.for_console()
    } else {
        app.theme.clone()
    };
    if app.linux_console {
        // The console keeps only the top bit of each channel for a background, so
        // every theme's dark selection shade turns black there: use its own blue.
        t.sel = ratatui::style::Color::Blue;
    }
    app.screen = (f.area().width, f.area().height);
    app.check_focus_mode();
    app.clear_view_rects();
    app.pins.board_hits.borrow_mut().clear();
    app.pins.board_preview.set(Rect::default());

    // In focus mode the menu can still appear over the first line of prose.
    app.bar_hits.borrow_mut().clear();
    let nonwriting_view = app.desk_view != DeskView::Write;
    let focused = app.focus_mode && !nonwriting_view;
    let [bar, main, info, progress] = Layout::vertical([
        Constraint::Length(u16::from(!focused)),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(u16::from(!focused)),
    ])
    .areas(f.area());
    app.rect_scene = Rect::default();
    app.view_hits.clear();
    app.widget_hits.clear();
    app.progress_hit = Rect::default();
    app.music_row_hits.clear();
    app.commonplace.side_area = Rect::default();

    if nonwriting_view {
        app.rect_tree = Rect::default();
        app.create_hits.clear();
        match app.desk_view {
            DeskView::Outline => crate::app::outline::draw(f, app, main, &t, false),
            DeskView::Study => crate::app::study::draw(f, app, main, &t),
            DeskView::Read => crate::app::read::draw(f, app, main, &t),
            _ => {}
        }
    } else {
        draw_write(f, app, main, &t);
    }
    let now = Instant::now();
    if focused {
        app.marquee = None;
        draw_focus_line(f, app, info, &t, now);
    } else {
        draw_info_bar(f, app, info, &t, now);
        draw_progress_bar(f, app, progress, &t, now);
    }
    let band = bar_band(&t, app.linux_console);
    if let Some(band) = band {
        paint_band(f.buffer_mut(), info, band);
        paint_band(f.buffer_mut(), progress, band);
    }
    if !focused {
        crate::app::overlays::draw_bar_line(f, app, bar, &t, true);
        if let Some(band) = band {
            paint_band(f.buffer_mut(), bar, band);
        }
    } else if matches!(
        app.overlay,
        Overlay::Bar { .. }
            | Overlay::Palette {
                under_bar: true,
                ..
            }
    ) {
        let top = Rect {
            height: 1,
            ..f.area()
        };
        crate::app::overlays::draw_bar_line(f, app, top, &t, false);
        if let Some(band) = band {
            paint_band(f.buffer_mut(), top, band);
        }
    }

    if !matches!(app.overlay, Overlay::None) {
        let mut overlay_area = f.area();
        if matches!(app.overlay, Overlay::Bar { .. }) {
            overlay_area.height = info.y.saturating_sub(overlay_area.y);
        }
        crate::app::overlays::draw(f, app, overlay_area, info, &t);
    }

    let animating = motion.is_some_and(|(motion, graphical)| {
        motion.paint(
            f.buffer_mut(),
            graphical,
            app.deck.state.low_power,
            crate::motion::Highlight::from_overlay(&app.overlay),
            &t,
            now,
        )
    });
    if t.is_rainbow() {
        paint_rainbow(f.buffer_mut(), t.accent, rainbow_phase());
    }
    animating
}

fn draw_write(f: &mut Frame, app: &mut App, main: Rect, t: &Theme) {
    let edit_area = if app.focus_mode {
        // Focus mode: the prose alone. No tree, clearing or music, and
        // nothing left behind for a click to land on.
        app.rect_tree = Rect::default();
        app.create_hits.clear();
        app.scene_visible = false;
        main
    } else if app.deck.state.current().outline && main.width >= MIN_LEFT + MIN_EDIT {
        draw_left(f, app, main, t)
    } else {
        app.rect_tree = Rect::default();
        app.create_hits.clear();
        app.scene_visible = false;
        if app.focus == Focus::Tree {
            app.focus = app.editor_focus();
        }
        main
    };

    let edit_area = crate::app::pins::sidebar(f, app, edit_area, t);
    let edit_area = if app.commonplace.side_visible && !app.focus_mode && edit_area.height >= 12 {
        let split = if edit_area.width >= 75 {
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        } else {
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
        };
        let [write, outline] = split.areas(edit_area);
        app.commonplace.side_area = outline;
        write
    } else {
        app.commonplace.select_side(false);
        edit_area
    };

    // Editable documents have independent contexts. Wide terminals put them
    // side by side; 80x24 remains useful by stacking prose vertically. Hidden
    // views retain their buffers but never receive Tab or mouse input.
    let secondary =
        !app.focus_mode && app.views.secondary_visible && app.view_open(Pane::Secondary).is_some();
    app.views.secondary_room = secondary && (edit_area.width >= 72 || edit_area.height >= 12);
    app.side_room = !app.focus_mode && !app.views.secondary_room && edit_area.width >= 70;
    app.rect_codex = Rect::default();
    app.rect_beside = Rect::default();
    if app.views.secondary_room {
        let percent = app.deck.state.current().secondary_percent;
        let parts = if edit_area.width >= 72 {
            Layout::horizontal([
                Constraint::Percentage(100 - percent),
                Constraint::Percentage(percent),
            ])
            .areas::<2>(edit_area)
        } else {
            Layout::vertical([
                Constraint::Percentage(100 - percent),
                Constraint::Percentage(percent),
            ])
            .areas::<2>(edit_area)
        };
        app.with_editor_view(Pane::Primary, |app| draw_editor(f, app, parts[0], t));
        app.with_editor_view(Pane::Secondary, |app| draw_editor(f, app, parts[1], t));
    } else {
        if app.focus == Focus::Secondary {
            app.activate_editor(Pane::Primary);
        }
        if app.side_room && (app.codex.is_some() || app.beside.is_some()) {
            let [ed, side] = Layout::horizontal([Constraint::Min(34), Constraint::Percentage(38)])
                .areas(edit_area);
            app.with_editor_view(Pane::Primary, |app| draw_editor(f, app, ed, t));
            if app.codex.is_some() {
                draw_codex(f, app, side, t);
            } else {
                draw_beside(f, app, side, t);
            }
        } else {
            app.with_editor_view(Pane::Primary, |app| draw_editor(f, app, edit_area, t));
        }
    }
    if matches!(app.focus, Focus::Codex | Focus::Beside) && !app.side_room {
        app.activate_editor(Pane::Primary);
    }
    if !app.commonplace.side_area.is_empty() {
        crate::app::outline::draw(f, app, app.commonplace.side_area, t, true);
    }
}

/// The ground of the bars, in the same colour as the pane borders. None on the console,
/// whose sixteen colours would turn it into a bright block.
pub(crate) fn bar_band(t: &Theme, linux_console: bool) -> Option<ratatui::style::Color> {
    (!linux_console && matches!(t.border, ratatui::style::Color::Rgb(..))).then_some(t.border)
}

/// Lays `band` under every cell of `area` that has no ground of its own, so a lit title or a
/// highlighted mark keeps its shade.
fn paint_band(buf: &mut ratatui::buffer::Buffer, area: Rect, band: ratatui::style::Color) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            if cell.bg == ratatui::style::Color::Reset {
                cell.set_bg(band);
            }
        }
    }
}

/// Seconds for the Rainbow theme's colours to flow once round the wheel:
/// lively enough to see move, slow enough to write beside.
pub(crate) const RAINBOW_CYCLE: f32 = 12.0;

/// Where the Rainbow theme's spectrum has flowed to by now, in degrees. Taken
/// from the clock, not the frame count, so the speed doesn't depend on how
/// often the screen is drawn.
pub(crate) fn rainbow_phase() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32();
    (t / RAINBOW_CYCLE).fract() * 360.0
}

/// The Rainbow theme's accent isn't one colour: every cell drawn in it — the
/// focused border, titles, the progress bar, the word count, the open scene —
/// takes its hue from where it sits on screen, a diagonal sweep across the
/// spectrum that flows round the wheel every [`RAINBOW_CYCLE`] seconds
/// (`phase` in degrees, from [`rainbow_phase`]).
pub(crate) fn paint_rainbow(
    buf: &mut ratatui::buffer::Buffer,
    accent: ratatui::style::Color,
    phase: f32,
) {
    let area = buf.area;
    let drift = phase;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            if cell.fg != accent && cell.bg != accent {
                continue;
            }
            let c = crate::theme::hue(x as f32 * 3.0 + y as f32 * 7.0 + drift);
            if cell.fg == accent {
                cell.fg = c;
            }
            if cell.bg == accent {
                cell.bg = c;
            }
        }
    }
}

/// The left column — tree and clearing — and what's left for writing.
fn draw_left(f: &mut Frame, app: &mut App, main: Rect, t: &Theme) -> Rect {
    let [left, edit_area] = Layout::horizontal([
        Constraint::Length(if app.deck.state.current().outline_width == LEFT_W {
            LEFT_W.min(main.width.saturating_sub(30)).max(MIN_LEFT)
        } else {
            app.deck
                .state
                .current()
                .outline_width
                .min(main.width.saturating_sub(30))
                .max(MIN_LEFT)
        }),
        Constraint::Min(MIN_EDIT),
    ])
    .areas(main);

    // Give up the clearing before the tree gets unusable.
    let (tree_area, scene_area) = if left.height >= SCENE_H + MIN_TREE {
        let [a, b] =
            Layout::vertical([Constraint::Min(MIN_TREE), Constraint::Length(SCENE_H)]).areas(left);
        (a, b)
    } else {
        (left, Rect::default())
    };

    app.scene_visible = scene_area.height > 0;

    draw_tree(f, app, tree_area, t);
    if app.scene_visible {
        // A Music view on screen is music asked for.
        if app.pane_mode == Mode::Music {
            app.music.wake();
        }
        draw_scene(f, app, scene_area, t);
    }
    edit_area
}

/// How a key looks in a popup's footer: lit, so it stands out from what it does.
pub(crate) fn key_style(t: &Theme) -> Style {
    wros_shell::look::key_style(t.into())
}

/// A popup footer like " y delete   esc keep it", its keys lit and the rest dim.
pub(crate) fn hint_line(text: &str, t: &Theme) -> Line<'static> {
    Line::from(hint_spans(text, t))
}

pub(crate) fn hint_spans(text: &str, t: &Theme) -> Vec<Span<'static>> {
    wros_shell::look::hint_spans(text, t.into())
}

pub(crate) fn pane_block<'a>(title: &'a str, focused: bool, t: &Theme) -> Block<'a> {
    wros_shell::look::pane_block(title, focused, t.into())
}

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    let focused = app.focus == Focus::Tree;
    // The book's own name heads the tree; Manuscript is a section inside it.
    let book = app.project.meta.title.trim().to_uppercase();
    let block = pane_block(if book.is_empty() { "UNTITLED" } else { &book }, focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.rect_tree = inner;

    let height = inner.height as usize;
    if app.sel < app.tree_scroll {
        app.tree_scroll = app.sel;
    } else if height > 0 && app.sel >= app.tree_scroll + height {
        app.tree_scroll = app.sel + 1 - height;
    }

    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    for (row, &idx) in app
        .visible
        .iter()
        .enumerate()
        .skip(app.tree_scroll)
        .take(height)
    {
        let n = &app.project.nodes[idx];
        let selected = row == app.sel;

        // ▾ ☺ Characters: a fold arrow for anything that folds, then the
        // thing's symbol, then its name.
        let indent = "  ".repeat(n.depth);
        let fold = match n.kind {
            Kind::Scene => "  ",
            _ if n.expanded => "▾ ",
            _ => "▸ ",
        };
        let open = Some(idx) == app.open;
        let (icon, icon_colour) = match n.kind {
            Kind::Category => (n.area.icon(), area_colour(t, n.area)),
            Kind::Container => ("▰", t.dim),
            Kind::Scene if n.area == Area::Format => (n.area.icon(), area_colour(t, n.area)),
            Kind::Scene if open => ("▮", t.accent),
            Kind::Scene => ("▯", t.dim),
        };

        // Counts are for writing: the manuscript and the notebook, not the
        // paperwork or the trash.
        let counted = n.area == Area::Manuscript || n.area.is_notebook();
        let words = if counted {
            app.project.subtree_words(idx)
        } else {
            0
        };
        // A parked copy or a file Grimoire won't write says so where the
        // count would be: two rows with the same name and different words is
        // exactly how the wrong one gets edited.
        let tag = if let Some(u) = &n.disk.unavailable {
            u.tag
        } else if n.read_only {
            "read-only"
        } else if let Some(of) = &n.copy_of {
            of.source.label()
        } else if n.parked {
            "parked copy"
        } else if n.clash {
            // wren.md beside Wren.md: one file on a Mac, in Box, on Windows
            "name clash"
        } else {
            ""
        };
        let count = if !tag.is_empty() {
            tag.to_string()
        } else if words > 0 {
            thousands(words)
        } else {
            String::new()
        };

        let lead = format!("{indent}{fold}");
        let icon = if app.icons_on {
            format!("{icon} ")
        } else {
            String::new()
        };
        let lead_w = lead.chars().count() + icon.chars().count();
        let room = width.saturating_sub(lead_w + count.chars().count() + 1);
        let title = truncate(&n.title, room.max(1));
        let gap = width
            .saturating_sub(lead_w + title.chars().count())
            .saturating_sub(count.chars().count() + 1)
            .max(1);

        let name_style = match n.kind {
            Kind::Category | Kind::Container => {
                Style::default().fg(t.text).add_modifier(Modifier::BOLD)
            }
            _ if open => Style::default().fg(t.accent),
            _ => Style::default().fg(t.text),
        };
        let dragging_to = app
            .tree_drag
            .is_some_and(|(from, to)| from != to && row == to);
        let base = if dragging_to {
            Style::default()
                .bg(t.sel)
                .add_modifier(Modifier::UNDERLINED)
        } else if selected && focused {
            Style::default().bg(t.sel)
        } else {
            Style::default()
        };

        lines.push(
            Line::from(vec![
                Span::styled(lead, Style::default().fg(t.dim).patch(base)),
                Span::styled(icon, Style::default().fg(icon_colour).patch(base)),
                Span::styled(title, name_style.patch(base)),
                Span::styled(" ".repeat(gap), base),
                Span::styled(
                    count,
                    Style::default()
                        .fg(if tag.is_empty() { t.dim } else { t.warn })
                        .patch(base),
                ),
                Span::styled(" ", base),
            ])
            .style(base),
        );
    }

    f.render_widget(Paragraph::new(lines), inner);
    draw_create_keys(f, app, area, focused, t);
}

/// The create keys, on the tree's bottom edge where the eye already is —
/// "n scene  c chapter  p part". Clicking one does what pressing it would.
fn draw_create_keys(f: &mut Frame, app: &mut App, area: Rect, focused: bool, t: &Theme) {
    app.create_hits.clear();
    if area.height < 3 || area.width < 4 {
        return;
    }
    let row = Rect {
        x: area.x + 1,
        y: area.y + area.height - 1,
        width: area.width - 2,
        height: 1,
    };
    let (key_style, word_style) = if focused {
        (
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            Style::default().fg(t.text),
        )
    } else {
        (Style::default().fg(t.dim), Style::default().fg(t.dim))
    };
    let sel = app.visible.get(app.sel).copied();
    let mut spans = vec![Span::raw(" ")];
    let mut x = row.x + 1;
    let offers = create::offers(&app.project, sel);
    for (i, (key, word)) in offers.iter().enumerate() {
        let gap = if i > 0 { 2 } else { 0 };
        let width = 2 + word.chars().count() as u16;
        if x + gap + width > row.x + row.width {
            break;
        }
        spans.push(Span::raw(" ".repeat(gap as usize)));
        x += gap;
        spans.push(Span::styled(key.to_string(), key_style));
        spans.push(Span::styled(format!(" {word}"), word_style));
        if let Some(want) = New::from_key(*key) {
            app.create_hits.push((
                Rect {
                    x,
                    y: row.y,
                    width,
                    height: 1,
                },
                want,
            ));
        }
        x += width;
    }
    spans.push(Span::raw(" "));
    f.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn draw_scene(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    if app.pane_mode == Mode::Music {
        draw_music_view(f, app, area, t);
        return;
    }
    // The pane's title says which view you're on, so ←/→ is discoverable without a
    // legend.
    let (label, grid) = match app.pane_mode {
        Mode::Pomodoro => (
            // Idle, the title has room to say where focus mode is.
            if app.pomo.phase == Phase::Idle && app.open.is_some() {
                format!("F2 timer · {}D focus mode", app.mod_label())
            } else {
                app.pomo.label()
            },
            scene::render(
                // A custom theme may borrow any preset's world.
                scene::Scenery::for_theme(t.world()),
                app.pomo.phase,
                app.pomo.progress(),
                app.pomo.phase != Phase::Idle && !app.pomo.running(),
                app.frame,
            ),
        ),
        Mode::Visualizer => crate::viz_view::view(app, t, area.width),
        Mode::Music => unreachable!("drawn by draw_music_view"),
    };

    let focused = app.focus == Focus::Clearing;
    let mut block = pane_block(&label, focused, t);
    // On a beat the frame flashes bloom, and fades back as the beat does.
    if app.pane_mode == Mode::Visualizer && app.viz.beat > 0.0 {
        let base = if focused { t.accent } else { t.border };
        block = block.border_style(Style::default().fg(blend(base, t.bloom, app.viz.beat * 0.5)));
    }
    let inner = block.inner(area);
    app.rect_scene = inner;
    f.render_widget(block, area);

    let night = app.pomo.phase == Phase::Break && app.pane_mode == Mode::Pomodoro;
    let lines: Vec<Line> = grid
        .iter()
        .map(|row| {
            Line::from(
                row.iter()
                    .map(|&(ch, ink)| {
                        let mut style = Style::default().fg(scene_colour(ink, t, night));
                        if let Ink::Lit(_, on) = ink {
                            style = style.bg(on);
                        }
                        Span::styled(ch.to_string(), style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
    draw_view_switch(f, app, area, focused, t);
}

/// The colour of one cell of the Pomodoro or the Visualizer: every world is
/// drawn in roles, so each theme paints it in its own palette.
pub(crate) fn scene_colour(ink: Ink, t: &Theme, night: bool) -> ratatui::style::Color {
    let lit = |v: u8| v as f32 / 255.0;
    match ink {
        Ink::Sky => t.border,
        Ink::Star => {
            if night {
                t.moon
            } else {
                t.dim
            }
        }
        Ink::Sun => t.sun,
        Ink::Moon => t.moon,
        Ink::Tree => t.foliage,
        Ink::Trunk => t.bark,
        Ink::Creature => t.text,
        Ink::Flower => t.bloom,
        Ink::Ground => t.turf,
        Ink::Water => blend(t.moon, t.border, 0.45),
        Ink::Foam => blend(t.text, t.moon, 0.35),
        Ink::Stone => blend(t.dim, t.border, 0.25),
        Ink::Snow => t.text,
        Ink::Light => t.sun,
        Ink::Glow(v) => blend(t.foliage, t.bloom, lit(v)),
        // Red outside to violet in, from whatever the theme calls warm,
        // sun, green, sky and flower — so any palette makes a rainbow.
        Ink::Band { n, faint, glint } => {
            let c = match n {
                0 => t.warn,
                1 => blend(t.warn, t.sun, 0.5),
                2 => t.sun,
                3 => t.foliage,
                _ => blend(t.moon, t.bloom, 0.4),
            };
            let c = if glint { blend(c, t.text, 0.55) } else { c };
            if faint { blend(c, t.border, 0.6) } else { c }
        }
        Ink::Accent => t.accent,
        Ink::Warn => t.warn,
        Ink::Cloud => blend(t.text, t.border, 0.55),
        Ink::Sand => blend(t.sun, t.bark, 0.5),
        Ink::Trail => t.accent,
        Ink::Dim => t.dim,
        // The Visualizer picks its colours itself (crate::viz_view).
        Ink::Paint(c) | Ink::Lit(c, _) => c,
    }
}

/// The view switcher, on the scene pane's bottom edge: "◂ pomodoro
/// visualizer ▸", the current view lit. It says ←/→ has somewhere to go, and
/// clicking a name or an arrow goes there.
fn draw_view_switch(f: &mut Frame, app: &mut App, area: Rect, focused: bool, t: &Theme) {
    if area.height < 3 || area.width < 4 {
        return;
    }
    let row = Rect {
        x: area.x + 1,
        y: area.y + area.height - 1,
        width: area.width - 2,
        height: 1,
    };
    let (items, spans) = view_switch(app.pane_mode, row.width);
    let arrow = Style::default().fg(if focused { t.accent } else { t.dim });
    let lit = Style::default()
        .fg(if focused { t.accent } else { t.text })
        .add_modifier(Modifier::BOLD);
    let styled: Vec<Span> = spans
        .into_iter()
        .map(|(text, kind)| match kind {
            Piece::Arrow => Span::styled(text, arrow),
            Piece::Current => Span::styled(text, lit),
            Piece::Other => Span::styled(text, Style::default().fg(t.dim)),
            Piece::Gap => Span::raw(text),
            Piece::Rule => Span::styled(
                text,
                Style::default().fg(if focused { t.accent } else { t.border }),
            ),
        })
        .collect();
    for (x, width, mode) in items {
        app.view_hits.push((
            Rect {
                x: row.x + x,
                y: row.y,
                width,
                height: 1,
            },
            mode,
        ));
    }
    f.render_widget(Paragraph::new(Line::from(styled)), row);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    Arrow,
    Current,
    Other,
    Gap,
    /// The pane's own border line, carried on either side of a centred
    /// switcher.
    Rule,
}

/// Lay out the switcher in `width` cells: the pieces to draw, and each
/// clickable one's (offset, width, view). Names drop from the right before
/// they'd run into the corner; the arrows always stay.
/// Clickable (offset, width, view) spans, and the labelled pieces to draw.
type Switcher = (Vec<(u16, u16, Mode)>, Vec<(String, Piece)>);

fn view_switch(current: Mode, width: u16) -> Switcher {
    let mut hits = vec![(0, 1, current.prev())];
    let mut spans = vec![("◂".to_string(), Piece::Arrow)];
    let mut x = 1u16;
    // Every name, a space before each, and " ▸".
    let all: u16 = Mode::ALL
        .iter()
        .map(|m| 1 + m.name().chars().count() as u16)
        .sum::<u16>()
        + 3;
    if all > width {
        // Too narrow for every name: the current one, and a dot for each view
        // ("◂ music · · ● ▸"), so the others still read as somewhere to go.
        let name = current.name();
        let w = name.chars().count() as u16;
        let dots = 2 * Mode::ALL.len() as u16;
        if x + 1 + w + dots + 2 <= width {
            spans.push((" ".into(), Piece::Gap));
            spans.push((name.into(), Piece::Current));
            hits.push((x + 1, w, current));
            x += 1 + w;
            for mode in Mode::ALL {
                spans.push((" ".into(), Piece::Gap));
                let (dot, kind) = if mode == current {
                    ("●", Piece::Current)
                } else {
                    ("·", Piece::Other)
                };
                spans.push((dot.into(), kind));
                hits.push((x + 1, 1, mode));
                x += 2;
            }
        }
    } else {
        for mode in Mode::ALL {
            let w = mode.name().chars().count() as u16;
            spans.push((" ".into(), Piece::Gap));
            let kind = if mode == current {
                Piece::Current
            } else {
                Piece::Other
            };
            spans.push((mode.name().into(), kind));
            hits.push((x + 1, w, mode));
            x += 1 + w;
        }
    }
    if x + 2 <= width {
        spans.push((" ".into(), Piece::Gap));
        spans.push(("▸".into(), Piece::Arrow));
        hits.push((x + 1, 1, current.next()));
        x += 2;
    }
    // Room to spare: centre the switcher on the edge, so it reads as a control
    // rather than a caption.
    let pad = width.saturating_sub(x) / 2;
    if pad > 0 {
        spans.insert(0, ("─".repeat(pad as usize), Piece::Rule));
        for h in &mut hits {
            h.0 += pad;
        }
    }
    (hits, spans)
}

/// A section's symbol colour. The manuscript takes the accent; the notebook
/// grows in foliage; the book's paperwork is sun; the trash stays dim.
fn area_colour(t: &Theme, a: Area) -> ratatui::style::Color {
    match a {
        Area::Manuscript => t.accent,
        Area::Characters | Area::Places | Area::Notes | Area::Research => t.foliage,
        Area::Format | Area::FrontMatter | Area::BackMatter | Area::Templates => t.sun,
        Area::Trash => t.dim,
    }
}

/// Mix two truecolour colours, `f` of the way from `a` to `b`. Anything that
/// isn't RGB just switches over halfway.
pub(crate) fn blend(
    a: ratatui::style::Color,
    b: ratatui::style::Color,
    f: f32,
) -> ratatui::style::Color {
    use ratatui::style::Color;
    let f = f.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
            Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
        }
        _ if f < 0.5 => a,
        _ => b,
    }
}

/// A spectrum bar's colour at height `h`, 0 at the roots to 255 at the tip:
/// the theme's foliage, up through its accent and sun, to bloom at the very
/// top. Drawn from the theme, so every palette gets its own.
pub(crate) fn bar_colour(t: &Theme, h: u8) -> ratatui::style::Color {
    let x = h as f32 / 255.0;
    // The rainbow's bars climb the spectrum itself: red at the roots, violet
    // at the tips.
    if t.is_rainbow() {
        return crate::theme::hue(x * 280.0);
    }
    let stops = [
        (0.0, t.foliage),
        (0.4, t.accent),
        (0.75, t.sun),
        (1.0, t.bloom),
    ];
    for w in stops.windows(2) {
        let ((a, from), (b, to)) = (w[0], w[1]);
        if x <= b {
            return blend(from, to, (x - a) / (b - a));
        }
    }
    t.bloom
}

/// The clearing's Music view (crate::music_view), with the switcher on its edge.
fn draw_music_view(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    let focused = app.focus == Focus::Clearing;
    let inner = pane_block("", focused, t).inner(area);
    let (title, lines, hits) = crate::music_view::view(app, t, inner.width, inner.height, focused);
    let block = pane_block(&title, focused, t);
    app.rect_scene = inner;
    f.render_widget(block, area);
    for (line, row) in hits {
        app.music_row_hits.push((
            Rect {
                x: inner.x,
                y: inner.y + line,
                width: inner.width,
                height: 1,
            },
            row,
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
    draw_view_switch(f, app, area, focused, t);
}

/// A word a check marked: underlined in `colour`. Where the terminal has no
/// underline colours (the Linux console), the word itself takes the colour.
fn marked(st: Style, colour: ratatui::style::Color, underline_colour: bool) -> Style {
    let st = st.add_modifier(Modifier::UNDERLINED);
    if underline_colour {
        st.underline_color(colour)
    } else {
        st.fg(colour)
    }
}

fn draw_editor(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    let focused = app.view_is_focused() && !app.desk_accepts_input();
    let mut title = app.open_title();
    if app.views.active == Pane::Secondary {
        title = format!("Secondary · {title}");
    }
    if let Some((words, target)) = app.scene_target() {
        title = format!("{title} · {} / {}", thousands(words), thousands(target));
    }
    let block = if app.focus_mode {
        // Focus mode draws the page, not a pane: no frame, no title.
        Block::default().padding(Padding::new(2, 2, 1, 0))
    } else {
        pane_block(&title, focused, t).padding(Padding::new(2, 2, 0, 0))
    };
    let inner = block.inner(area);
    app.rect_prose = area;
    f.render_widget(block, area);

    // Prose is set in a column no wider than the line width, centred.
    let col = prose_column(inner, app.line_width);
    app.rect_editor = col;
    app.edit_width = col.width as usize;
    app.edit_height = col.height as usize;

    if app.open.is_none() {
        let empty_manuscript = !app
            .project
            .nodes
            .iter()
            .any(|n| n.in_manuscript && n.kind == Kind::Scene);
        let hint = vec![
            Line::from(""),
            Line::from(Span::styled(
                if empty_manuscript {
                    "Esc › Project › New scene… starts a scene."
                } else {
                    "Select a scene on the left and press Enter."
                },
                Style::default().fg(t.dim),
            )),
        ];
        f.render_widget(Paragraph::new(hint), inner);
        return;
    }

    let rows = app.editor.layout(app.edit_width);
    app.follow_caret(&rows);

    // Each row is painted in layers, char by char, then merged into runs:
    // find matches, then the selection on top.
    let find = match &app.overlay {
        Overlay::Find { query, .. } if !query.is_empty() => Some(query.as_str()),
        _ => None,
    };
    let check_spans = app.check_line_spans();
    let match_bg = blend(t.border, t.sun, 0.45);
    let mut line_matches: std::collections::HashMap<usize, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    let mut line_spelling: std::collections::HashMap<usize, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    let mut line_names: std::collections::HashMap<usize, Vec<(usize, usize, usize)>> =
        std::collections::HashMap::new();
    // Notes and TKs, which may run across lines, and echoing words when on.
    let marks = grimoire_core::notes::line_spans(&app.editor.lines);
    let echoes = if app.echo_on {
        grimoire_core::revision::echoes(&app.editor.lines, &app.echo_skip())
    } else {
        Vec::new()
    };
    let echo_bg = blend(t.border, t.moon, 0.35);
    let visible: Vec<Line> = rows
        .iter()
        .skip(app.editor.scroll)
        .take(app.edit_height)
        .map(|&r| {
            let plain = Style::default().fg(t.text);
            let chars: Vec<char> = app.editor.lines[r.line]
                .chars()
                .skip(r.start)
                .take(r.end - r.start)
                .collect();
            let mut styles = vec![plain; chars.len()];
            let mut paint = |from: usize, to: usize, f: &dyn Fn(Style) -> Style| {
                for c in from.max(r.start)..to.min(r.end) {
                    styles[c - r.start] = f(styles[c - r.start]);
                }
            };
            // Notebook names in the accent colour.
            let names = line_names.entry(r.line).or_insert_with(|| {
                grimoire_core::codex::spans(&app.editor.lines[r.line], &app.codex_index)
            });
            for &(s, e, _) in names.iter() {
                paint(s, e, &|st| st.fg(t.accent));
            }
            // Notes to yourself, quiet; TKs, gaps to come back to, in the sun.
            for &(s, e, kind) in &marks[r.line] {
                match kind {
                    grimoire_core::notes::MarkKind::Note => {
                        paint(s, e, &|st| st.fg(t.dim).add_modifier(Modifier::ITALIC))
                    }
                    grimoire_core::notes::MarkKind::Tk => {
                        paint(s, e, &|st| st.fg(t.sun).add_modifier(Modifier::BOLD))
                    }
                }
            }
            if let Some(ec) = echoes.get(r.line) {
                for &(s, e) in ec {
                    paint(s, e, &|st| st.bg(echo_bg));
                }
            }
            if let Some(q) = find {
                let hits = line_matches.entry(r.line).or_insert_with(|| {
                    grimoire_core::search::matches_with(&app.editor.lines[r.line], q, app.find_opts)
                });
                for &(s, e) in hits.iter() {
                    paint(s, e, &|st| st.bg(match_bg));
                }
            }
            // Current-revision grammar is a separate, quieter underline.
            // Scalar columns came from checked original body byte spans once
            // per prose view. Spelling retains its own layer and dictionary.
            if let Some(spans) = check_spans.get(&r.line) {
                for &(s, e) in spans {
                    paint(s, e, &|st| marked(st, t.moon, !app.linux_console));
                }
            }
            // Misspellings: a warn-coloured underline, never on the word being typed.
            if app.spell_on {
                let bad = line_spelling.entry(r.line).or_insert_with(|| {
                    App::misspellings_outside_marks(app.misspellings(r.line), &marks[r.line])
                });
                for &(s, e) in bad.iter() {
                    paint(s, e, &|st| marked(st, t.warn, !app.linux_console));
                }
            }
            if let Some((from, to)) = app.editor.row_selection(r) {
                paint(from, to, &|st| {
                    st.bg(t.sel)
                        .fg(if find.is_some() { t.accent } else { t.text })
                });
            }
            let mut spans: Vec<Span> = Vec::new();
            let mut run = String::new();
            let mut run_style = plain;
            for (ch, st) in chars.iter().zip(styles) {
                if st != run_style && !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), run_style));
                }
                run_style = st;
                run.push(*ch);
            }
            if !run.is_empty() {
                spans.push(Span::styled(run, run_style));
            }
            Line::from(spans)
        })
        .collect();

    // An empty scene shows where the words go. It is only drawn, never
    // part of the text, and the first keystroke replaces it.
    let empty = app.editor.lines.len() == 1 && app.editor.lines[0].is_empty();
    if empty {
        let cue = if focused {
            "Start writing · Esc for the menu"
        } else {
            "Tab or click to write · Esc for the menu"
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(cue, col.width as usize),
                Style::default().fg(t.dim).add_modifier(Modifier::ITALIC),
            ))),
            col,
        );
    } else {
        f.render_widget(Paragraph::new(visible), col);
    }

    if focused && matches!(app.overlay, Overlay::None) {
        let (r, c) = app.editor.cursor_vis(&rows);
        if r >= app.editor.scroll && r < app.editor.scroll + app.edit_height {
            let x = col.x + c.min(app.edit_width.saturating_sub(1)) as u16;
            let y = col.y + (r - app.editor.scroll) as u16;
            f.set_cursor_position((x, y));
        }
    }
}

/// The column prose is set in: `width` wide at most (0: all of it),
/// centred in `inner`.
fn prose_column(inner: Rect, width: usize) -> Rect {
    let w = match width {
        0 => inner.width,
        w => inner.width.min(w.min(u16::MAX as usize) as u16),
    };
    Rect {
        x: inner.x + (inner.width - w) / 2,
        width: w,
        ..inner
    }
}

/// Another scene, read-only, beside the one being written.
fn draw_beside(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    let Some(i) = app.beside_node() else {
        // Moved or deleted since it was opened.
        app.close_beside();
        return;
    };
    let focused = app.focus == Focus::Beside;
    let n = &app.project.nodes[i];
    let title = format!("{} · read-only", n.title);
    let block = pane_block(&title, focused, t).padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.rect_beside = area;

    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for para in n.body.split('\n') {
        if para.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        for l in wrap_words(para, w) {
            lines.push(Line::from(Span::styled(l, Style::default().fg(t.text))));
        }
    }
    if n.body.trim().is_empty() {
        lines = vec![Line::from(Span::styled(
            "Nothing written here yet.",
            Style::default().fg(t.dim),
        ))];
    }
    let max = lines.len().saturating_sub(inner.height as usize);
    let Some(pane) = &mut app.beside else { return };
    pane.scroll = pane.scroll.min(max);
    let scroll = pane.scroll;
    f.render_widget(
        Paragraph::new(lines.into_iter().skip(scroll).collect::<Vec<_>>()),
        inner,
    );
}

fn draw_codex(f: &mut Frame, app: &mut App, area: Rect, t: &Theme) {
    let Some(pane) = &app.codex else { return };
    let focused = app.focus == Focus::Codex;
    let title = if pane.entry.section.is_empty() {
        pane.entry.title.clone()
    } else {
        format!("{} · {}", pane.entry.title, pane.entry.section)
    };
    let block = pane_block(&title, focused, t).padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.rect_codex = inner;
    let Some(pane) = &app.codex else { return };
    let dim = Style::default().fg(t.dim);
    let w = inner.width as usize;

    // The appearances list takes the bottom; the note fills the rest.
    let (list_area, visible) = pane.appearances_layout(inner);
    let note_area = Rect {
        height: inner.height - list_area.height,
        ..inner
    };

    let body = app
        .project
        .nodes
        .iter()
        .find(|n| n.path == pane.entry.note)
        .map(|n| n.body.clone())
        .unwrap_or_default();
    let mut note: Vec<Line> = Vec::new();
    for para in body.split('\n') {
        if para.trim().is_empty() {
            note.push(Line::from(""));
            continue;
        }
        for l in wrap_words(para, w) {
            note.push(Line::from(Span::styled(l, Style::default().fg(t.text))));
        }
    }
    let scroll = pane.scroll.min(note.len().saturating_sub(1));
    f.render_widget(
        Paragraph::new(note.into_iter().skip(scroll).collect::<Vec<_>>()),
        note_area,
    );

    let mut list = vec![Line::from(Span::styled(
        match pane.appears.len() {
            0 => "Not in the manuscript yet".to_string(),
            1 => "Appears in 1 scene".to_string(),
            n => format!("Appears in {n} scenes"),
        },
        Style::default().fg(t.accent),
    ))];
    let open_path = app.open.map(|i| app.project.nodes[i].path.clone());
    for i in visible {
        let a = &pane.appears[i];
        let here = open_path.as_ref() == Some(&a.scene);
        let on = focused && i == pane.sel;
        let count = format!(" ×{}", a.count);
        let place = truncate(&a.place, w.saturating_sub(count.chars().count() + 2));
        let row = Line::from(vec![
            Span::styled(if here { "● " } else { "  " }, Style::default().fg(t.sun)),
            Span::styled(
                place,
                Style::default().fg(if on { t.accent } else { t.text }),
            ),
            Span::styled(count, dim),
        ]);
        list.push(if on {
            row.style(Style::default().bg(t.sel))
        } else {
            row
        });
    }
    f.render_widget(Paragraph::new(list), list_area);
}

pub(crate) fn info_section(app: &App) -> Vec<Span<'static>> {
    match app.desk_view {
        DeskView::Read => vec![Span::raw(crate::app::read::info(app))],
        DeskView::Outline => crate::app::outline::info(app)
            .map(Span::raw)
            .into_iter()
            .collect(),
        DeskView::Study => vec![Span::raw(crate::app::study::info(app))],
        DeskView::Write if shows_scene_path(app) => scene_path(app),
        DeskView::Write => {
            if let Some(selection) = app.editor.selected_text() {
                let words = grimoire_core::notes::count_words(&selection);
                return vec![Span::raw(format!("{} words selected", thousands(words)))];
            }
            let Some(scene) = app.open.map(|i| &app.project.nodes[i]) else {
                return Vec::new();
            };
            let mut details = Vec::new();
            details.extend(
                scene
                    .status
                    .iter()
                    .filter(|s| !s.trim().is_empty())
                    .cloned(),
            );
            if let Some(goal) = scene.meta("target").and_then(|value| value.parse().ok()) {
                details.push(format!("goal {}", thousands(goal)));
            }
            if let Some(modified) = scene.stat.and_then(|stat| stat.modified) {
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                details.push(format!("edited {} min ago", age.as_secs() / 60));
            }
            if details.is_empty() {
                Vec::new()
            } else {
                vec![Span::raw(details.join(info_separator(app.linux_console)))]
            }
        }
    }
}

fn shows_scene_path(app: &App) -> bool {
    app.desk_view == DeskView::Write
        && (app.focus_mode || !app.deck.state.current().outline || app.rect_tree.is_empty())
}

fn scene_path(app: &App) -> Vec<Span<'static>> {
    let Some(index) = app.open else {
        return Vec::new();
    };
    let scene = &app.project.nodes[index];
    let arrow = breadcrumb_separator(app.linux_console);
    let dot = info_separator(app.linux_console);
    let mut prefix = format!("{}{arrow}", app.project.meta.title);
    if let Some(chapter) = app.parents[index].map(|i| &app.project.nodes[i])
        && chapter.kind == Kind::Container
    {
        prefix.push_str(&chapter.title);
        prefix.push_str(arrow);
    }
    vec![
        Span::raw(prefix),
        Span::raw(format!("{}{dot}{}", scene.title, thousands(scene.words()))),
    ]
}

pub(crate) fn info_separator(console: bool) -> &'static str {
    if console { " | " } else { " · " }
}

pub(crate) fn breadcrumb_separator(console: bool) -> &'static str {
    if console { " > " } else { " › " }
}

fn ellipsis(console: bool) -> &'static str {
    if console { "..." } else { "…" }
}

/// Shorten at a character boundary using terminal cells, including wide titles.
fn fit_text(text: &str, width: usize, from_left: bool, console: bool) -> String {
    if Span::raw(text).width() <= width {
        return text.to_owned();
    }
    let ellipsis = ellipsis(console);
    let Some(room) = width.checked_sub(Span::raw(ellipsis).width()) else {
        return ".".repeat(width);
    };
    if from_left {
        let first = text
            .char_indices()
            .rev()
            .map(|(i, _)| i)
            .take_while(|&i| Span::raw(&text[i..]).width() <= room)
            .last()
            .unwrap_or(text.len());
        format!("{ellipsis}{}", &text[first..])
    } else {
        let end = text
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|&i| Span::raw(&text[..i]).width() <= room)
            .last()
            .unwrap_or_default();
        format!("{}{ellipsis}", &text[..end])
    }
}

fn info_text(info: &[Span<'_>]) -> String {
    info.iter().map(|span| span.content.as_ref()).collect()
}

fn draw_info_bar(f: &mut Frame, app: &mut App, area: Rect, t: &Theme, now: Instant) {
    if area.is_empty() {
        return;
    }
    let info = info_section(app);
    let path = shows_scene_path(app);
    let console = app.linux_console;
    let text = info_text(&info);
    let width = usize::from(area.width);
    let path_tail = if path {
        info.last().map_or(0, Span::width) + Span::raw(ellipsis(console)).width()
    } else {
        0
    };
    let text_width = Span::raw(text.as_str()).width();
    let reserved = text_width.min((width / 2).max(path_tail));
    let light = app
        .deck
        .light_flash
        .is_some_and(|at| now.saturating_duration_since(at) < gauges::LIGHT_SHOWN);
    let items = gauges::fit(
        app.deck
            .gauges
            .items(false, console, light, &app.deck.clock),
        width.saturating_sub(reserved + 1),
    );
    let gauge_width: usize = items.iter().map(|(_, s)| gauges::item_width(s)).sum();
    let left = fit_text(&text, width.saturating_sub(gauge_width + 1), path, console);
    f.render_widget(
        Line::styled(format!(" {left}"), Style::default().fg(t.text)),
        area,
    );

    let mut x = area.right() - gauge_width as u16;
    let dim = Style::default().fg(t.dim);
    for (item, text) in items {
        let width = (gauges::item_width(&text) - 2) as u16;
        let rect = Rect::new(x + 1, area.y, width, 1);
        if let Some(widget) = item.widget() {
            app.widget_hits.push((rect, widget));
        }
        x += width + 2;
        let line = match (item, app.deck.gauges.wifi) {
            (Item::Wifi, Some(gauges::Wifi::Bars(n))) if !console => Line::from(
                text.chars()
                    .enumerate()
                    .map(|(i, c)| {
                        let style = if i < usize::from(n) {
                            Style::default().fg(t.text)
                        } else {
                            dim
                        };
                        Span::styled(c.to_string(), style)
                    })
                    .collect::<Vec<_>>(),
            ),
            (Item::Battery, _) if app.deck.gauges.low_battery() => {
                Line::styled(text, Style::default().fg(t.warn))
            }
            _ => Line::styled(text, dim),
        };
        f.render_widget(line, rect);
    }
}

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

fn draw_progress_bar(f: &mut Frame, app: &mut App, area: Rect, t: &Theme, now: Instant) {
    if area.is_empty() {
        app.marquee = None;
        return;
    }
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

    let mut message = if let Some(battery) = app.deck.battery.showing(now) {
        Some(Span::styled(battery, Style::default().fg(t.warn)))
    } else if let Some((says, _)) = app.bar_description() {
        Some(Span::styled(says, dim.add_modifier(Modifier::ITALIC)))
    } else {
        (!app.msg.is_empty()).then_some(Span::styled(app.msg.as_str(), accent))
    };
    let track = match &app.music.state {
        crate::music::State::Playing(track) if track.playing && message.is_none() => {
            Some(track.title.trim()).filter(|title| !title.is_empty())
        }
        _ => None,
    };

    let width = usize::from(area.width);
    // A message is something to read now, so it wins room: the bar shrinks away first, then the
    // total and target give way (today's count stays), and only a message too long even for that
    // takes the whole line, until it clears.
    if let Some(shown) = &message {
        let today = daily.width() + sprint.as_ref().map_or(0, Span::width);
        let beside_all = width.saturating_sub(count.width() + goal.width() + today + 3);
        let beside_today = width.saturating_sub(today + 3);
        if shown.width() > beside_all {
            app.marquee = None;
            app.progress_hit = Rect::default();
            if shown.width() <= beside_today {
                let mut left = vec![daily.clone()];
                left.extend(sprint.clone());
                f.render_widget(Line::from(left), area);
                let right = Rect::new(
                    area.right() - 1 - shown.width() as u16,
                    area.y,
                    shown.width() as u16,
                    1,
                );
                f.render_widget(Line::styled(shown.content.clone(), shown.style), right);
            } else {
                let text = fit_text(
                    &shown.content,
                    width.saturating_sub(2),
                    false,
                    app.linux_console,
                );
                f.render_widget(Line::styled(format!(" {text}"), shown.style), area);
            }
            return;
        }
    }
    let counts_width = count.width() + goal.width();
    let room =
        width.saturating_sub(counts_width + daily.width() + sprint.as_ref().map_or(0, Span::width));
    let music_prefix = if app.linux_console { "Music " } else { "♪ " };
    let prefix_width = Span::raw(music_prefix).width();
    let right_width = if let Some(message) = &message {
        message.width()
    } else {
        track.map_or(0, |title| {
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
    let filled = (total.min(target) as u128 * bar_width as u128 / target as u128) as usize;
    let (written, remaining) = if app.linux_console {
        ("=", "-")
    } else {
        ("━", "─")
    };
    app.progress_hit = Rect::new(
        area.x,
        area.y,
        (counts_width + bar_width).min(width) as u16,
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
    f.render_widget(Line::from(progress), area);

    let title_room = right_room.saturating_sub(prefix_width);
    if let Some(title) = track.filter(|_| title_room > 0) {
        let shown = if !app.deck.state.low_power && Span::raw(title).width() > title_room {
            if app
                .marquee
                .as_ref()
                .is_some_and(|m| !m.matches(title, title_room))
            {
                app.marquee = None;
            }
            let marquee = app
                .marquee
                .get_or_insert_with(|| Marquee::new(title, title_room, now));
            marquee.advance(now);
            marquee.visible().to_owned()
        } else {
            app.marquee = None;
            fit_text(title, title_room, false, app.linux_console)
        };
        message = Some(Span::styled(format!("{music_prefix}{shown}"), dim));
    } else {
        app.marquee = None;
    }
    if let Some(message) = message.filter(|_| right_room > 0) {
        let text = fit_text(&message.content, right_room, false, app.linux_console);
        let right = Rect::new(
            area.right() - 1 - right_room as u16,
            area.y,
            right_room as u16,
            1,
        );
        f.render_widget(Line::styled(text, message.style), right);
    }
}

fn draw_focus_line(f: &mut Frame, app: &App, area: Rect, t: &Theme, now: Instant) {
    let info = info_section(app);
    let mut tail = vec![daily_words(app, t)];
    for (text, style) in status_marks(app, t, true) {
        tail.push(Span::styled(format!("  {text}"), style));
    }
    let warn = Style::default().fg(t.warn);
    let battery = app
        .deck
        .gauges
        .items(true, app.linux_console, false, "")
        .into_iter()
        .next()
        .map(|(_, text)| Span::styled(format!("  {text}"), warn));
    let tail_width =
        tail.iter().map(Span::width).sum::<usize>() + battery.as_ref().map_or(0, Span::width);
    let room = usize::from(area.width).saturating_sub(1 + tail_width);
    let path_tail = info.last().map_or(0, Span::width);
    let notice = app.deck.battery.showing(now).and_then(|text| {
        let notice_room = room.saturating_sub(path_tail + 3);
        (notice_room > 0).then(|| {
            let shown = fit_text(text, notice_room, false, app.linux_console);
            Span::styled(format!("  {shown}"), warn)
        })
    });
    let path = fit_text(
        &info_text(&info),
        room.saturating_sub(notice.as_ref().map_or(0, Span::width)),
        true,
        app.linux_console,
    );
    let mut spans = vec![Span::styled(
        format!(" {path}"),
        Style::default().fg(t.accent),
    )];
    spans.extend(tail);
    spans.extend(notice);
    spans.extend(battery);
    f.render_widget(Line::from(spans), area);
}

/// The marks at the right of the menu bar's line: conflicts waiting to be
/// settled, whether the words are saved, and the book's backup. With
/// `warnings_only`, just the ones that need attention (focus mode).
pub(crate) fn status_marks(app: &App, t: &Theme, warnings_only: bool) -> Vec<(String, Style)> {
    ranked_marks(app, t, warnings_only)
        .into_iter()
        .map(|(text, style, _)| (text, style))
        .collect()
}

/// How much a mark matters when the line is short: words not saved most,
/// then conflicts, a failed backup, words waiting, and the rest.
const UNSAVED: u8 = 4;
const CONFLICT: u8 = 3;
const TROUBLE: u8 = 2;
const PENDING: u8 = 1;
const CALM: u8 = 0;

fn ranked_marks(app: &App, t: &Theme, warnings_only: bool) -> Vec<(String, Style, u8)> {
    let console = app.linux_console;
    let mut out = vec![];
    let conflicts = app.conflicts().len();
    if conflicts > 0 {
        out.push((
            if console {
                format!(
                    "{conflicts} conflict{}",
                    if conflicts == 1 { "" } else { "s" }
                )
            } else {
                format!("⚠ {conflicts}")
            },
            Style::default().fg(t.warn),
            CONFLICT,
        ));
    }
    // Saved is a steady dim tick; saving, waiting and not saved say so.
    let dirty = app.project.dirty_count();
    let save = match &app.save_state {
        crate::app::SaveState::Failed(_) if dirty > 0 => Some((
            if console {
                format!("{dirty} not saved")
            } else {
                format!("● {dirty}")
            },
            Style::default().fg(t.warn),
            UNSAVED,
        )),
        // Not on disk yet: focus mode says so too.
        crate::app::SaveState::Waiting(_) if dirty > 0 => Some((
            if console {
                format!("{dirty} waiting")
            } else {
                format!("◌ {dirty}")
            },
            Style::default().fg(t.sun),
            PENDING,
        )),
        _ if dirty > 0 => Some((
            if console {
                "saving".into()
            } else {
                "○".into()
            },
            Style::default().fg(t.dim),
            CALM,
        )),
        _ => Some((
            if console {
                "saved".into()
            } else {
                "✓".into()
            },
            Style::default().fg(t.dim),
            CALM,
        )),
    };
    if let Some((text, style, rank)) = save
        && (rank > CALM || !warnings_only)
    {
        out.push((text, style, rank));
    }
    if let Some(mark) = app.backup_mark() {
        use crate::app::BackupMark;
        let (text, style, rank) = match mark {
            BackupMark::Done(at) => (
                if console {
                    format!("backed up {at}")
                } else {
                    format!("⇅ {at}")
                },
                Style::default().fg(t.dim),
                CALM,
            ),
            BackupMark::Running => (
                if console {
                    "backing up".into()
                } else {
                    "⇅…".into()
                },
                Style::default().fg(t.accent),
                CALM,
            ),
            BackupMark::Waiting => (
                if console {
                    "backup waiting".into()
                } else {
                    "⇅◌".into()
                },
                Style::default().fg(t.sun),
                CALM,
            ),
            BackupMark::Failed => (
                if console {
                    "backup failed".into()
                } else {
                    "⇅!".into()
                },
                Style::default().fg(t.warn),
                TROUBLE,
            ),
        };
        if rank > CALM || !warnings_only {
            out.push((text, style, rank));
        }
    }
    out
}

/// Draw the marks between the logo and Power. Return where the titles must stop.
pub(crate) fn draw_status_marks(f: &mut Frame, app: &App, line: Rect, t: &Theme) -> u16 {
    let mut marks = ranked_marks(app, t, false);
    let mut gap = "  ";
    while !marks.is_empty() {
        let width: u16 = marks
            .iter()
            .map(|(s, _, _)| (s.chars().count() + gap.len()) as u16)
            .sum::<u16>()
            + 1;
        if width <= line.width {
            let mut spans = vec![];
            for (text, style, _) in marks {
                spans.push(Span::raw(gap));
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw(" "));
            let area = Rect::new(line.right() - width, line.y, width, 1);
            f.render_widget(Paragraph::new(Line::from(spans)), area);
            return area.x.saturating_sub(2).max(line.x);
        }
        let least = marks.iter().map(|(_, _, rank)| *rank).min().unwrap_or(CALM);
        if least >= CONFLICT && gap == "  " {
            // When the titles and other marks are gone, shorten the warnings first.
            gap = " ";
            for (text, _, rank) in &mut marks {
                *text = match (*rank == UNSAVED, app.linux_console) {
                    (true, true) => "unsaved",
                    (false, true) => "conflict",
                    (true, false) => "●",
                    (false, false) => "⚠",
                }
                .into();
            }
        } else if let Some(at) = marks.iter().rposition(|(_, _, rank)| *rank == least) {
            marks.remove(at);
        }
    }
    line.right().saturating_sub(1).max(line.x)
}

// ── overlays ─────────────────────────────────────────────────────────

pub(crate) fn centred(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Greedy word wrap for short card text.
pub(crate) fn wrap_words(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let need = if cur.is_empty() {
            word.chars().count()
        } else {
            cur.chars().count() + 1 + word.chars().count()
        };
        if need > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|l| truncate(&l, width)).collect()
}

pub(crate) fn truncate(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        return s.to_string();
    }
    let mut out: String = s.chars().take(w.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kept version reads against now, never as a loss: "6 fewer" looked
    /// like six words had gone missing.

    #[test]
    fn a_wide_switcher_names_all_three_views() {
        let (hits, spans) = view_switch(Mode::Visualizer, 40);
        let text: String = spans.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(
            text.trim_start_matches('─'),
            "◂ pomodoro visualizer music ▸"
        );
        assert!(text.chars().count() as u16 <= 40);
        let pad = text.chars().take_while(|c| *c == '─').count() as u16;
        let lit: Vec<&str> = spans
            .iter()
            .filter(|(_, k)| *k == Piece::Current)
            .map(|(s, _)| s.as_str())
            .collect();
        assert_eq!(lit, ["visualizer"]);
        // Arrows step from the current view; each name goes to itself.
        let at = |x: u16| {
            hits.iter()
                .find(|(o, w, _)| x >= *o && x < o + w)
                .map(|h| h.2)
        };
        assert_eq!(at(pad), Some(Mode::Pomodoro), "◂");
        assert_eq!(at(pad + 2), Some(Mode::Pomodoro), "pomodoro");
        assert_eq!(at(pad + 11), Some(Mode::Visualizer), "visualizer");
        assert_eq!(at(pad + 22), Some(Mode::Music), "music");
        assert_eq!(at(pad + 28), Some(Mode::Music), "▸");
        assert_eq!(at(pad + 1), None);
    }

    #[test]
    fn inside_the_clearing_the_switcher_shows_the_view_and_a_dot_for_each() {
        let inner = LEFT_W - 2;
        let (hits, spans) = view_switch(Mode::Music, inner);
        let text: String = spans.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(text.trim_start_matches('─'), "◂ music · · ● ▸");
        assert!(text.chars().count() as u16 <= inner);
        let pad = text.chars().take_while(|c| *c == '─').count() as u16;
        let at = |x: u16| {
            hits.iter()
                .find(|(o, w, _)| x >= *o && x < o + w)
                .map(|h| h.2)
        };
        assert_eq!(at(pad), Some(Mode::Visualizer), "◂");
        assert_eq!(at(pad + 8), Some(Mode::Pomodoro), "first dot");
        assert_eq!(at(pad + 12), Some(Mode::Music), "lit dot");
        assert_eq!(at(pad + 14), Some(Mode::Pomodoro), "▸");
    }

    #[test]
    fn a_very_narrow_switcher_keeps_its_arrows() {
        let (_, spans) = view_switch(Mode::Pomodoro, 8);
        let text: String = spans.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(text.trim_start_matches('─'), "◂ ▸");
    }
}
