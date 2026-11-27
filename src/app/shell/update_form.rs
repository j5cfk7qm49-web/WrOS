//! Offline update folder entry and cached package plan.

use super::{App, Choice, Entry, Key, Route};

pub(in crate::app) struct UpdateBundleForm {
    pub(super) bundle_dir: String,
    pub(super) selected: usize,
}
impl UpdateBundleForm {
    fn rows(&self) -> Vec<String> {
        vec![
            format!("Update folder on your media: {}", self.bundle_dir),
            "Cancel and return without reviewing".into(),
            "Review update · checks the signature, installs nothing".into(),
            "Use a folder under /run/wros-update/media, /media, /run/media or /mnt.".into(),
            "The review verifies the WrOS signature, the package list and every .deb checksum."
                .into(),
            "Nothing is installed until you confirm on a later screen.".into(),
            "Create an encrypted backup first to keep your projects safe.".into(),
        ]
    }
}

pub(super) fn bundle_rows(app: &App) -> Vec<Entry> {
    app.deck
        .shell
        .update_form
        .as_ref()
        .map(|form| form.rows().into_iter().map(Entry::info).collect())
        .unwrap_or_default()
}

pub(super) fn package_plan_rows(app: &App) -> Vec<Entry> {
    let mut rows = vec![
        Entry::new(
            "Refresh cached package plan",
            Choice::Read(wros_sys::Request::CheckUpdates),
        ),
        Entry::new(
            "Review a signed update bundle by path",
            Choice::UpdateBundleOpen,
        ),
        Entry::new(
            "Recovery status, repair and staged update",
            Choice::View(Route::RecoveryView, wros_sys::Request::RecoveryStatus),
        ),
        Entry::info("Preview only. No package download or installation is performed."),
        Entry::info("Repository freshness and recovery are unchecked."),
    ];
    if let Some(plan) = &app.deck.shell.updates {
        rows.push(Entry::info(&plan.status));
        rows.extend(
            plan.packages
                .iter()
                .map(|name| Entry::info(format!("Install/upgrade: {name}"))),
        );
        rows.extend(
            plan.removals
                .iter()
                .map(|name| Entry::info(format!("Remove: {name}"))),
        );
    } else {
        rows.push(Entry::info(
            "Cached package plan unavailable; refresh to try again.",
        ));
    }
    rows
}

impl App {
    pub(crate) fn paste_update_form(&mut self, text: &str) {
        let Some(form) = &mut self.deck.shell.update_form else {
            return;
        };
        if form.selected != 0 {
            return;
        }
        if form.bundle_dir.len() + text.len() <= 512 && !text.chars().any(char::is_control) {
            form.bundle_dir.push_str(text);
        }
    }

    pub(super) fn on_update_form_key(&mut self, key: Key) {
        let Some(form) = &mut self.deck.shell.update_form else {
            return;
        };
        match key {
            Key::Home => form.selected = 0,
            Key::Esc => {
                self.deck.shell.update_form = None;
                self.deck.shell.nav.back();
            }
            Key::Tab | Key::Down => form.selected = (form.selected + 1) % 3,
            Key::Up => {
                form.selected = if form.selected == 0 {
                    2
                } else {
                    form.selected - 1
                };
            }
            Key::Backspace if form.selected == 0 => {
                form.bundle_dir.pop();
            }
            Key::Delete if form.selected == 0 => {
                form.bundle_dir.clear();
            }
            Key::Char(c) if !c.is_control() && form.selected == 0 => {
                self.paste_update_form(&c.to_string());
            }
            Key::Enter if form.selected == 0 => {
                form.selected = 2;
            }
            Key::Enter if form.selected == 1 => {
                self.deck.shell.update_form = None;
                self.deck.shell.nav.back();
            }
            Key::Enter if form.selected == 2 => {
                if self.deck.busy {
                    return;
                }
                let bundle_dir = form.bundle_dir.trim().to_string();
                if let Err(err) = wros_sys::system_views::validate_update_bundle_path(&bundle_dir) {
                    self.deck.note = format!("Unavailable: {err}");
                    self.deck.shell.notices.push(&self.deck.note);
                    return;
                }
                self.deck.shell.update_flow.last_result = None;
                self.deck
                    .request(wros_sys::Request::ReviewUpdateBundle { bundle_dir });
            }
            _ => {}
        }
    }
}
