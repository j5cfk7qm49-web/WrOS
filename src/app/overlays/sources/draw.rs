use super::state::{
    CatalogueState, FIELDS, FORM_FIELD_START, FORM_SEARCH, FORM_TYPE, Page, SourceDialog, TYPES,
    field_value, text,
};
use crate::catalogue::{self, Hit, SearchStatus};
use crate::theme::Theme;
use crate::ui::{centred, hint_line, pane_block, truncate};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};

pub(in crate::app) fn catalogue_row(hit: &Hit, width: usize) -> String {
    let title = text(&hit.record, "title");
    let author = field_value(Some(&hit.record), "author");
    let publisher = text(&hit.record, "publisher");
    let issued = field_value(Some(&hit.record), "issued");
    let year = issued.split('-').next().unwrap_or_default();
    let isbn = text(&hit.record, "ISBN");

    let mut end = String::new();
    if !isbn.is_empty() {
        end.push_str(" · ISBN");
    } else {
        if matches!(text(&hit.record, "type"), "article" | "article-journal") {
            end.push_str(" · article");
        }
        if !text(&hit.record, "DOI").is_empty() {
            end.push_str(" · DOI");
        }
    }
    let edition_count = hit
        .editions
        .filter(|&count| count > 1)
        .map_or_else(String::new, |count| format!(" · {count} editions →"));
    let details = [title, &author, publisher, year]
        .iter()
        .map(|part| part.chars().count())
        .sum::<usize>();
    if !isbn.is_empty()
        && details + end.chars().count() + edition_count.chars().count() + isbn.len() + 9 <= width
    {
        end.push(' ');
        end.push_str(isbn);
    }
    end.push_str(&edition_count);
    if !publisher.is_empty() || !year.is_empty() {
        let room = width
            .saturating_sub(end.chars().count() + year.chars().count() + 22)
            .max(1);
        let publisher = truncate(publisher, room);
        let details = format!("{publisher} {year}");
        end.insert_str(0, &format!(" · {}", details.trim()));
    }
    let room = width.saturating_sub(end.chars().count()).max(1);
    let head = if author.is_empty() || room < 9 {
        truncate(title, room)
    } else {
        let title = truncate(title, room - 6);
        let author = truncate(&author, room - title.chars().count() - 3);
        format!("{title} · {author}")
    };
    format!("{head}{end}")
}

fn picker_row(label: &str, selected: bool, width: usize, active: Style) -> Line<'static> {
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let label = truncate(&label, width);
    let marker = if selected { "›" } else { " " };
    Line::styled(
        format!("{marker} {label}"),
        if selected { active } else { Style::default() },
    )
}

fn catalogue_status(catalogue: &CatalogueState) -> Option<&str> {
    if catalogue.resolving.is_some() {
        return Some("Resolving author names…");
    }
    match catalogue.search.status() {
        SearchStatus::Offline => Some(catalogue::OFFLINE),
        SearchStatus::Error(error) => Some(error),
        _ => None,
    }
}

fn catalogue_header(catalogue: &CatalogueState) -> Line<'static> {
    let online = if catalogue.search.status() == &SearchStatus::Off {
        "off"
    } else {
        "on"
    };
    Line::from(format!("Catalogue · online search {online}"))
}

fn draw_editions(
    dialog: &SourceDialog,
    selected: usize,
    inner: Rect,
    active: Style,
) -> Vec<Line<'static>> {
    let Some(catalogue) = &dialog.catalogue else {
        return Vec::new();
    };
    let catalogue = catalogue.0.borrow();
    let Some((work, editions, loading)) = catalogue.search.editions() else {
        return Vec::new();
    };
    let width = usize::from(inner.width.saturating_sub(2));
    let mut lines = vec![
        catalogue_header(&catalogue),
        Line::styled(
            truncate(
                &format!("Editions · {}", text(&work.record, "title")),
                width,
            ),
            active,
        ),
    ];
    let status = catalogue_status(&catalogue).or(loading.then_some("Loading editions…"));
    let room = usize::from(inner.height)
        .saturating_sub(4 + status.is_some() as usize)
        .max(1);
    let first = (selected + 1).saturating_sub(room);
    for (i, hit) in editions.iter().enumerate().skip(first).take(room) {
        let label = catalogue_row(hit, width);
        lines.push(picker_row(&label, i == selected, width, active));
    }
    if let Some(status) = status {
        lines.push(Line::from(truncate(status, width)));
    } else if editions.is_empty() {
        lines.push(Line::from("No editions found."));
    }
    lines
}

pub(in crate::app::overlays) fn draw(f: &mut Frame, dialog: &SourceDialog, area: Rect, t: &Theme) {
    f.render_widget(ProjectSources { dialog, theme: t }, area);
}

struct ProjectSources<'a> {
    dialog: &'a SourceDialog,
    theme: &'a Theme,
}

impl Widget for ProjectSources<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Self { dialog, theme: t } = self;
        let width = if matches!(dialog.page, Page::Pick { .. }) {
            100
        } else {
            68
        };
        let height = if matches!(dialog.page, Page::Form { .. }) {
            24
        } else {
            20
        };
        let r = centred(area, width.min(area.width), height.min(area.height));
        Clear.render(r, buf);
        let title = match &dialog.page {
            Page::List => "Project sources",
            Page::OutlinePick(_) => "Link a Commonplace edition",
            Page::EditionChanges { .. } => "Details changed. Update?",
            Page::Remove(_) => "Delete source",
            Page::Restore { .. } => "Restore project sources",
            Page::Drive { .. } => "Resolve source conflict",
            Page::Pick { .. } => "Cite a source",
            Page::Form {
                original_key: Some(_),
                ..
            } => "Edit source",
            Page::Form { .. } => "Add a source",
            Page::Import { .. } => "Import sources",
        };
        let block = pane_block(title, true, t);
        let inner = block.inner(r);
        block.render(r, buf);
        let active = Style::default().fg(t.accent);
        let mut lines = Vec::new();
        let help = match &dialog.page {
            Page::EditionChanges {
                changes, scroll, ..
            } => {
                if let Some(change) = changes.front() {
                    lines.push(Line::styled(
                        format!("{} · {} to review", change.project_key, changes.len()),
                        active,
                    ));
                    let fields: std::collections::BTreeSet<_> = change
                        .before
                        .keys()
                        .chain(change.proposed.keys())
                        .filter(|key| {
                            !key.starts_with("x-wros")
                                && change.before.get(*key) != change.proposed.get(*key)
                        })
                        .collect();
                    let width = usize::from(inner.width);
                    let mut preview = Vec::new();
                    for field in fields {
                        preview.push(format!(
                            "{field} · {}",
                            field_value(Some(&change.before), field)
                        ));
                        preview.push(format!(
                            "  → {}",
                            field_value(Some(&change.proposed), field)
                        ));
                    }
                    if preview.is_empty() {
                        preview.push("Your local details stay as they are.".into());
                    }
                    let room = usize::from(inner.height.saturating_sub(5).max(1));
                    let scroll = (*scroll).min(preview.len().saturating_sub(room));
                    lines.extend(
                        preview
                            .iter()
                            .skip(scroll)
                            .take(room)
                            .map(|line| Line::from(truncate(line, width))),
                    );
                }
                "Enter update   K keep accepted   ↑/↓ scroll   Esc keep all"
            }
            Page::Remove(key) => {
                lines.push(Line::from(format!(
                    "Remove {key} from this project's sources?"
                )));
                "Enter delete   Esc cancel"
            }
            Page::Restore {
                missing, scroll, ..
            } => {
                if missing.is_empty() {
                    lines.push(Line::from(
                        "Every citation in this project has a source in this version.",
                    ));
                } else {
                    lines.push(Line::from(
                        "This version leaves these citation keys without a source:",
                    ));
                    let room = inner.height.saturating_sub(5).max(1) as usize;
                    let start = (*scroll).min(missing.len().saturating_sub(room));
                    lines.extend(
                        missing
                            .iter()
                            .skip(start)
                            .take(room)
                            .map(|id| Line::from(id.as_str())),
                    );
                }
                "↑/↓ preview   Enter restore   Esc cancel"
            }
            Page::Drive {
                merge,
                choices,
                index,
                scroll,
            } => {
                if let Some(id) = merge.conflicts().get(*index) {
                    lines.push(Line::from(format!(
                        "Source {id} · {} of {}",
                        index + 1,
                        merge.conflicts().len()
                    )));
                    let mut preview = Vec::new();
                    for (label, drive) in [("This desk", false), ("Drive", true)] {
                        preview.push(format!(
                            "{label}{}",
                            if choices.get(id) == Some(&drive) {
                                " [selected]"
                            } else {
                                ""
                            }
                        ));
                        let json = merge
                            .record(id, drive)
                            .and_then(|record| serde_json::to_string_pretty(record).ok())
                            .unwrap_or_else(|| "(deleted)".into());
                        preview.extend(json.lines().map(str::to_owned));
                    }
                    let room = inner.height.saturating_sub(5).max(1) as usize;
                    let start = (*scroll).min(preview.len().saturating_sub(room));
                    lines.extend(
                        preview
                            .iter()
                            .skip(start)
                            .take(room)
                            .map(|l| Line::from(truncate(l, inner.width as usize))),
                    );
                    "← keep desk   → keep Drive   ↑/↓ scroll   Esc cancel"
                } else {
                    lines.push(Line::from(format!(
                        "{} conflicting sources reviewed.",
                        choices.len()
                    )));
                    "Enter save   Shift-Tab back   Esc cancel"
                }
            }
            Page::Pick { chosen: None, .. } | Page::OutlinePick(_) | Page::Form { .. }
                if dialog.catalogue_view.is_some() =>
            {
                lines = draw_editions(dialog, dialog.catalogue_view.unwrap_or(0), inner, active);
                "↑/↓ choose   Enter select   ←/Esc back"
            }
            Page::List | Page::OutlinePick(_) | Page::Pick { chosen: None, .. } => {
                let catalogue = dialog
                    .picker_catalogue()
                    .map(|catalogue| catalogue.0.borrow());
                let width = usize::from(inner.width.saturating_sub(2));
                lines.push(Line::styled(format!("Find [{}]", dialog.query), active));
                if let Page::Pick { cluster, .. } = &dialog.page
                    && !cluster.is_empty()
                {
                    lines.push(Line::from(format!("{} in this citation", cluster.len())));
                }
                let matches = dialog.matches();
                let status = catalogue.as_deref().and_then(catalogue_status);
                let room = if catalogue.is_some() {
                    usize::from(inner.height)
                        .saturating_sub(lines.len() + status.is_some() as usize + 3)
                        .max(1)
                } else {
                    usize::from(inner.height.saturating_sub(6).max(1))
                };
                let first = (dialog.selected + 1).saturating_sub(room);
                let mut visible_local = 0;
                for (i, (key, record)) in matches.iter().enumerate().skip(first).take(room) {
                    let label = format!(
                        "{key} · {} · {}",
                        field_value(Some(record), "author"),
                        text(record, "title")
                    );
                    lines.push(picker_row(&label, i == dialog.selected, width, active));
                    visible_local += 1;
                }
                if matches.is_empty()
                    && catalogue
                        .as_ref()
                        .is_none_or(|catalogue| catalogue.search.status() == &SearchStatus::Off)
                {
                    lines.push(Line::from(
                        "No sources match. Add or import sources from Tools › Notes.",
                    ));
                }
                if let Some(catalogue) = catalogue.as_deref() {
                    lines.push(catalogue_header(catalogue));
                    if let Some(status) = status {
                        lines.push(Line::from(truncate(status, width)));
                    }
                    for (i, hit) in catalogue
                        .search
                        .hits()
                        .iter()
                        .enumerate()
                        .skip(first.saturating_sub(matches.len()))
                        .take(room - visible_local)
                    {
                        lines.push(picker_row(
                            &catalogue_row(hit, width),
                            i + matches.len() == dialog.selected,
                            width,
                            active,
                        ));
                    }
                }
                if matches!(dialog.page, Page::List) {
                    "↑/↓ choose   Enter edit   Delete remove   Esc close"
                } else if catalogue
                    .as_ref()
                    .is_some_and(|catalogue| catalogue.search.status() != &SearchStatus::Off)
                {
                    "↑/↓ choose   → editions   Enter select   Esc cancel"
                } else {
                    "↑/↓ choose   Enter select   Esc cancel"
                }
            }
            Page::Pick {
                chosen: Some(cite),
                field,
                ..
            } => {
                lines.push(Line::from(cite.key.as_str()));
                lines.push(Line::from(truncate(
                    &cite.works_cited,
                    usize::from(inner.width),
                )));
                for (i, label) in [
                    format!("Page [{}]", cite.page),
                    format!(
                        "[{}] Author named in my sentence",
                        if cite.suppress { "x" } else { " " }
                    ),
                    "Add another source".into(),
                ]
                .into_iter()
                .enumerate()
                {
                    lines.push(Line::styled(
                        label,
                        if i == *field {
                            active
                        } else {
                            Style::default()
                        },
                    ));
                }
                "Tab field   Space check   E edit   Enter confirm   Esc cancel"
            }
            Page::Form {
                original_key,
                kind,
                fields,
                field,
                ..
            } => {
                let searching = *field == FORM_SEARCH;
                let width = usize::from(inner.width);
                if original_key.is_none()
                    && let Some(catalogue) = &dialog.catalogue
                {
                    let catalogue = catalogue.0.borrow();
                    lines.push(Line::styled(
                        truncate(&format!("Search [{}]", dialog.query), width),
                        if searching { active } else { Style::default() },
                    ));
                    lines.push(catalogue_header(&catalogue));
                    if let Some(status) = catalogue_status(&catalogue) {
                        lines.push(Line::from(truncate(status, width)));
                    }
                    if searching {
                        let width = width.saturating_sub(2);
                        let room = usize::from(inner.height)
                            .saturating_sub(lines.len() + 4)
                            .min(5);
                        let first = (dialog.selected + 1).saturating_sub(room);
                        for (i, hit) in catalogue
                            .search
                            .hits()
                            .iter()
                            .enumerate()
                            .skip(first)
                            .take(room)
                        {
                            lines.push(picker_row(
                                &catalogue_row(hit, width),
                                i == dialog.selected,
                                width,
                                active,
                            ));
                        }
                    }
                }
                let type_name = if original_key.is_some() {
                    kind.as_str()
                } else {
                    TYPES
                        .iter()
                        .find_map(|&(label, csl)| (csl == kind.as_str()).then_some(label))
                        .unwrap_or(kind.as_str())
                };
                lines.push(Line::styled(
                    truncate(&format!("Type [{type_name}]"), width),
                    if *field == FORM_TYPE {
                        active
                    } else {
                        Style::default()
                    },
                ));
                let room = usize::from(inner.height)
                    .saturating_sub(lines.len() + 1 + usize::from(!dialog.notice.is_empty()))
                    .max(1);
                for (i, ((label, _), entry)) in FIELDS
                    .iter()
                    .zip(fields)
                    .enumerate()
                    .skip(field.saturating_sub(room + FORM_FIELD_START - 1))
                    .take(room)
                {
                    let row = format!("{label} [{entry}]");
                    lines.push(Line::styled(
                        truncate(&row, width),
                        if i + FORM_FIELD_START == *field {
                            active
                        } else {
                            Style::default()
                        },
                    ));
                }
                if searching {
                    "↑/↓ choose  → editions  Enter search/pick  Tab form  Esc cancel"
                } else {
                    "←/→ type   Tab field   Enter save   Esc cancel"
                }
            }
            Page::Import {
                path,
                preview,
                scroll,
            } => {
                if let Some((_, notes)) = preview {
                    let room = inner.height.saturating_sub(4).max(1) as usize;
                    let first = (*scroll).min(notes.len().saturating_sub(room));
                    lines.extend(
                        notes
                            .iter()
                            .skip(first)
                            .take(room)
                            .map(|note| Line::from(truncate(note, inner.width as usize))),
                    );
                    "↑/↓ preview   Enter import   Esc cancel"
                } else {
                    lines.push(Line::styled(
                        format!(".bib or CSL-JSON path [{path}]"),
                        active,
                    ));
                    "Enter preview   Esc cancel"
                }
            }
        };
        if !dialog.notice.is_empty() {
            lines.push(Line::styled(
                truncate(&dialog.notice, inner.width as usize),
                Style::default().fg(t.warn),
            ));
        }
        lines.push(hint_line(help, t));
        Paragraph::new(lines).render(inner, buf);
    }
}
