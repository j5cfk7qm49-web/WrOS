//! The Settings menu map and navigation through its pages.

use super::pages::entries;
use super::{App, Category, Choice, Entry, Key, Overlay, Panel, Route, SettingsRow, music, wros};
use grimoire_core::tutorial;

/// The Settings page each menu-bar row is chosen on.
fn settings_row_page(row: SettingsRow) -> Route {
    match row {
        SettingsRow::Sleep | SettingsRow::Restart | SettingsRow::ShutDown => {
            Route::Panel(Panel::Power)
        }
        SettingsRow::Install => Route::Panel(Panel::Storage),
        SettingsRow::BackUpNow => Route::Backups,
    }
}

/// The Settings menu map (#90): each group page and the pages under it, in order.
/// Group pages, their rows, breadcrumbs and the paths the tutorial opens all come
/// from here, so moving a page to another group is moving its line. Each page still
/// builds its own rows in `entries`.
const MENU: &[(Route, &[Route])] = &[
    (
        Route::Settings,
        &[
            Route::Category(Category::Writing),
            Route::Category(Category::Interface),
            Route::Category(Category::System),
            Route::Category(Category::Accounts),
            Route::Category(Category::Updates),
            Route::Category(Category::Music),
        ],
    ),
    (
        Route::Category(Category::Writing),
        &[
            Route::WritingEditor,
            Route::WritingManuscript,
            Route::WritingGoals,
            Route::WritingRevision,
        ],
    ),
    (
        Route::Category(Category::Interface),
        &[Route::LookPage, Route::KeyboardPage],
    ),
    // Battery and power stays third: the lock VM test reaches it by position.
    (
        Route::Category(Category::System),
        &[
            Route::Panel(Panel::Network),
            Route::Panel(Panel::Display),
            Route::Panel(Panel::Power),
            Route::SoundView,
            Route::Panel(Panel::DateTime),
            Route::Panel(Panel::Storage),
        ],
    ),
    (
        Route::Category(Category::Accounts),
        &[Route::Backups, Route::GoogleAccount, Route::LocalAccounts],
    ),
    (Route::GoogleAccount, &[Route::GoogleDrive]),
    (
        Route::Category(Category::Updates),
        &[Route::Panel(Panel::Updates), Route::Panel(Panel::Recovery)],
    ),
    (Route::Category(Category::Music), &[Route::MusicPage]),
    (
        Route::Backups,
        &[
            Route::BackupDestinations,
            Route::BackupKey,
            Route::BackupQueue,
        ],
    ),
];

/// The map, for tests that walk it.
#[cfg(test)]
pub(crate) fn menu() -> &'static [(Route, &'static [Route])] {
    MENU
}

/// A page's name in its group's list and in breadcrumbs.
pub(crate) fn page_label(route: Route) -> &'static str {
    match route {
        Route::Settings => "Settings",
        Route::Category(category) => category.label(),
        Route::Panel(panel) => panel.label(),
        Route::WritingEditor => "Editor",
        Route::WritingManuscript => "Manuscript",
        Route::WritingGoals => "Goals",
        Route::WritingRevision => "Revision",
        Route::LookPage => "Look",
        Route::KeyboardPage => "Keyboard",
        Route::MusicPage => "Music",
        Route::SoundView => "Sound and volume",
        Route::Backups => "Backups",
        Route::GoogleAccount => "Google Drive",
        Route::GoogleDrive => "Your Drive files",
        Route::LocalAccounts => "Local accounts",
        Route::ChangePassword => "Change password",
        Route::BackupDestinations => "Where backups go",
        Route::BackupKey => "Recovery key",
        Route::BackupQueue => "Offline queue",
        _ => "",
    }
}

/// The group a page sits in, from the map.
pub(crate) fn parent(route: Route) -> Option<Route> {
    MENU.iter()
        .find(|(_, pages)| pages.contains(&route))
        .map(|(group, _)| *group)
}

/// The groups and page to open, from Settings down, for `route`.
pub(crate) fn path_to(route: Route) -> Vec<Route> {
    let mut path = vec![route];
    while let Some(group) = parent(path[0]).filter(|group| *group != Route::Settings) {
        path.insert(0, group);
    }
    path
}

/// `WrOS · Settings › System › Display`, for a page on the map.
pub(super) fn breadcrumb(route: Route) -> Option<String> {
    (route == Route::Settings || parent(route).is_some()).then(|| {
        std::iter::once("WrOS · Settings")
            .chain(
                path_to(route)
                    .into_iter()
                    .filter(|r| *r != Route::Settings)
                    .map(page_label),
            )
            .collect::<Vec<_>>()
            .join(" › ")
    })
}

/// One row per page under `group`. Pages that load something first ask for it as they open.
pub(super) fn links(group: Route) -> Vec<Entry> {
    MENU.iter()
        .find(|(page, _)| *page == group)
        .map_or(&[][..], |(_, pages)| pages)
        .iter()
        .map(|&page| {
            let choice = match page {
                Route::SoundView => Choice::View(page, wros_sys::Request::AudioStatus),
                Route::LocalAccounts => Choice::View(page, wros_sys::Request::ListLocalAccounts),
                page => Choice::Open(page),
            };
            Entry::new(page_label(page), choice)
        })
        .collect()
}

impl App {
    /// Open a Settings page and do what choosing it does: fetch the status it shows.
    pub(in crate::app) fn open_route(&mut self, route: Route) {
        if route == Route::MusicServerForm {
            self.open_music_server_form();
            return;
        }
        self.deck.shell.nav.open(route);
        if route == Route::MusicPage {
            self.refresh_music_file_counts();
        }
        if route == Route::GoogleDisconnectConfirm {
            self.deck.shell.nav.select(0, 4);
        }
        if matches!(route, Route::Backups | Route::GoogleAccount) {
            self.deck
                .shell
                .backup
                .request(crate::backup_service::Request::Status);
        }
        let availability: Option<&str> = None;
        if let Some(message) = availability {
            self.deck.note = message.into();
            self.deck.shell.notices.push(message);
        }
        if route == Route::WarmScreen && !self.deck.busy {
            self.deck.request(wros_sys::Request::DisplayStatus);
        }
        if route == Route::Panel(Panel::Display)
            && self.deck.boot_settings_available
            && !self.deck.busy
        {
            self.deck.request(wros_sys::Request::BootStatus);
        }
        if matches!(route, Route::Panel(Panel::Updates | Panel::Recovery)) {
            // These panels show what the update helper reports,
            // never a cached guess.
            if !self.deck.busy {
                self.refresh_recovery();
            }
        } else if matches!(route, Route::Panel(_))
            && self.deck.snapshot.is_none()
            && !self.deck.busy
        {
            self.deck.request(wros_sys::Request::Refresh);
        }
    }

    pub(in crate::app) fn refresh_music_file_counts(&mut self) {
        if self.music.source == music::Source::Files {
            self.deck.shell.music_file_counts = music::file_counts();
        }
    }

    /// Settings, already on the page at the end of `path` (the pages before it are
    /// what Esc returns through).
    /// Opens a page on the Settings menu map, through its groups, so Back retraces them.
    pub(crate) fn open_settings_page(&mut self, page: Route) {
        self.open_settings_at(&path_to(page));
    }
    pub(crate) fn open_settings_at(&mut self, path: &[Route]) {
        self.open_settings();
        if let Some((last, parents)) = path.split_last() {
            for route in parents {
                self.deck.shell.nav.open(*route);
            }
            self.open_route(*last);
        }
    }
    /// Choose a row of a Settings page as if the writer had: the WrOS menu's
    /// confirmed power rows and File › Back up now run what Settings runs, with
    /// the same confirmations, and leave the Settings box on that page.
    pub(in crate::app) fn choose_in_settings(&mut self, row: SettingsRow) {
        let was_busy = self.deck.busy;
        self.deck.shell.pending_row = None;
        self.open_settings_page(settings_row_page(row));
        if !was_busy && self.deck.busy {
            // Opening the page started reading the system status (the first time since
            // login), and its rows refuse while that runs: choose once it is read.
            self.deck.shell.pending_row = Some(row);
            return;
        }
        self.choose_on_open_page(row);
    }
    /// Choose the row the menu bar asked for while its page was loading, once it has
    /// loaded, if the writer is still on that page.
    pub(in crate::app) fn run_pending_settings_row(&mut self) {
        if self.deck.busy {
            return;
        }
        let Some(row) = self.deck.shell.pending_row.take() else {
            return;
        };
        if self.overlay == Overlay::Deck(wros::Page::Settings)
            && self.deck.shell.nav.current() == settings_row_page(row)
        {
            self.choose_on_open_page(row);
        }
    }
    fn choose_on_open_page(&mut self, row: SettingsRow) {
        let wanted = |choice: &Choice| match row {
            SettingsRow::Sleep => matches!(choice, Choice::System(8)),
            SettingsRow::Restart => matches!(choice, Choice::System(10)),
            SettingsRow::ShutDown => matches!(choice, Choice::System(9)),
            SettingsRow::Install => matches!(choice, Choice::InstallerOpen),
            SettingsRow::BackUpNow => matches!(choice, Choice::BackupNow),
        };
        let rows = entries(self);
        if let Some(i) = rows.iter().position(|e| wanted(&e.choice)) {
            self.deck.shell.nav.select(i, rows.len());
            self.on_settings_key(Key::Enter);
        } else if row == SettingsRow::Sleep {
            // Sleep has no row on the page; it asks the same way the System page does.
            self.deck.shell.external = true;
            self.overlay = Overlay::Deck(wros::Page::System { sel: 8 });
            self.on_deck_key(Key::Enter);
            self.return_to_settings();
        }
    }
    pub(crate) fn open_settings(&mut self) {
        self.deck.shell.external = false;
        self.deck.shell.wifi_form = None;
        self.deck.shell.music_server_form = None;
        self.deck.shell.music_setup = None;
        self.deck.shell.wifi_saved_choice = None;
        self.deck.shell.update_form = None;
        self.deck.shell.clock_form = None;
        self.deck.shell.clock_choice = None;
        self.deck.shell.keyboard_wizard = None;
        let flow = &mut self.deck.shell.update_flow;
        flow.review = None;
        flow.dir = None;
        flow.recheck = None;
        flow.stage_error = None;
        flow.changed = None;
        self.deck.shell.nav.reset(Route::Settings);
        self.overlay = Overlay::Deck(wros::Page::Settings);
        self.observe_deck(tutorial::Event::SystemOpened);
    }

    pub(in crate::app) fn return_to_settings(&mut self) {
        if self.deck.shell.external
            && matches!(
                self.overlay,
                Overlay::None | Overlay::Bar { .. } | Overlay::Deck(wros::Page::System { .. })
            )
        {
            self.deck.shell.external = false;
            self.overlay = Overlay::Deck(wros::Page::Settings);
        }
    }
}
