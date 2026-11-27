//! Settings page rows and their selectable entries.

#[cfg(feature = "test-driver")]
use super::navigation::breadcrumb;
use super::navigation::links;
use super::{
    App, Category, Choice, Entry, MusicServerForm, PasswordForm, Route, accounts, display, music,
    system, update_flow, update_form, wifi,
};
use crate::palette::Action;

pub(super) fn entries(app: &App) -> Vec<Entry> {
    page_entries(app, app.deck.shell.nav.current())
}

/// The rows of the Settings page `route`.
fn page_entries(app: &App, route: Route) -> Vec<Entry> {
    match route {
        route @ (Route::Settings | Route::Category(_)) => {
            let mut rows = links(route);
            match route {
                Route::Category(Category::Writing) => {
                    rows.extend(setting_rows(app, &[Action::Voices]))
                }
                Route::Category(Category::Interface) => rows.push(Entry::info(
                    "Accessibility and notifications · more controls planned",
                )),
                _ => {}
            }
            rows
        }
        Route::WritingEditor => {
            let mut rows = setting_rows(
                app,
                &[
                    Action::Spellcheck,
                    Action::SpellingLanguage,
                    Action::LineWidth,
                    Action::Typewriter,
                    Action::PomodoroChime,
                ],
            );
            rows.push(Entry::new("Focus mode", Choice::Action(Action::FocusMode)));
            rows.extend(setting_rows(app, &[Action::TaughtWords]));
            rows
        }
        Route::WritingManuscript => {
            let mut rows = setting_rows(
                app,
                &[
                    Action::ManuscriptLook,
                    Action::AuthorDetails,
                    Action::MlaPaperDetails,
                    Action::CatalogueSearch,
                ],
            );
            rows.push(Entry::new(
                "Footnotes · browse and jump",
                Choice::Action(Action::Footnotes),
            ));
            rows
        }
        Route::WritingGoals => vec![
            Entry::new("Writing goals", Choice::Action(Action::Goals)),
            Entry::new("Daily challenges", Choice::Action(Action::DailyChallenges)),
            Entry::new("Draft milestones", Choice::Action(Action::Milestones)),
        ],
        Route::WritingRevision => vec![
            Entry::new("Revision tools", Choice::Action(Action::ProseDashboard)),
            Entry::new("Continuity tracker", Choice::Action(Action::Continuity)),
            Entry::new("Revision notes", Choice::Action(Action::ReviewNotes)),
        ],
        Route::LookPage => {
            let mut rows = setting_rows(
                app,
                &[
                    Action::Themes,
                    Action::Icons,
                    Action::Workspaces,
                    Action::ToggleOutline,
                ],
            );
            if app.deck.boot_settings_available {
                rows.push(Entry::info(
                    "The start screen takes the new colours at the next restart.",
                ));
            }
            rows
        }
        Route::KeyboardPage => display::keyboard_rows(),
        Route::MusicPage => {
            let mut rows = vec![Entry::new(
                format!("Source · {}", app.music.source.label()),
                Choice::Action(Action::MusicSource),
            )];
            rows.extend(music_source_rows(app));
            rows
        }
        Route::MusicServerForm => {
            match (
                &app.deck.shell.music_server_form,
                &app.deck.shell.music_setup,
            ) {
                (Some(form), _) => form.rows().into_iter().map(Entry::info).collect(),
                (_, Some(job)) => vec![Entry::info(job.status)],
                _ => Vec::new(),
            }
        }
        Route::Panel(panel) => system::panel_rows(app, panel),
        Route::WifiNetworks => wifi::network_rows(app),
        Route::WifiConnect => wifi::connect_rows(app),
        Route::ChangePassword => accounts::change_password_rows(app),
        Route::SavedWifi => wifi::saved_rows(app),
        Route::SavedWifiProfile => wifi::saved_profile_rows(app),
        Route::LocalAccounts => accounts::local_accounts_rows(app),
        Route::StorageDevicesView => system::storage_devices_rows(app),
        Route::UpdatesView => update_form::package_plan_rows(app),
        Route::UpdateBundleForm => update_form::bundle_rows(app),
        Route::UpdateReview => update_flow::review_rows(app),
        Route::UpdateConfirm => update_flow::confirm_rows(app),
        Route::UpdateStaged => update_flow::staged_rows(app),
        Route::GoBackConfirm => update_flow::go_back_rows(app),
        Route::RepairConfirm => update_flow::repair_rows(),
        Route::GoogleAccount => accounts::google_account_rows(app),
        Route::LinkPhone => accounts::link_phone_rows(app),
        Route::GoogleDisconnectConfirm => accounts::google_disconnect_rows(),
        Route::GoogleDrive => accounts::drive_rows(app),
        Route::SyncPanel => accounts::sync_rows(app),
        Route::Backups => accounts::backup_rows(app),
        Route::BackupDestinations => accounts::backup_destination_rows(app),
        Route::BackupKey => accounts::backup_key_rows(app),
        Route::BackupQueue => accounts::backup_queue_rows(app),
        Route::BackupForm => accounts::backup_form_rows(app),
        Route::RecoveryView => update_flow::recovery_rows(app),
        Route::Timezones => system::timezone_rows(app),
        Route::TimezoneConfirm => system::timezone_confirm_rows(app),
        Route::WarmScreen => display::warm_screen_rows(app),
        Route::InstallerConfirm => system::installer_confirm_rows(),
        Route::WarmPreview => display::warm_preview_rows(app),
        Route::ManualClock => system::clock_form_rows(app),
        Route::ManualClockConfirm => system::clock_confirm_rows(app),
        Route::SoundView => system::sound_rows(app),
        Route::KeyboardWizard => display::keyboard_wizard_rows(app),
    }
}

/// Forms list their fields and buttons as descriptions and move their own
/// cursor; the first `options` rows are the ones it moves among.
fn form_options(app: &App) -> Option<usize> {
    let shell = &app.deck.shell;
    match shell.nav.current() {
        Route::BackupForm => shell.backup.form.as_ref().map(|f| f.field_count() + 1),
        Route::WifiConnect => shell.wifi_form.as_ref().map(|f| f.action_count()),
        Route::ChangePassword => shell.password_form.as_ref().map(|_| PasswordForm::ACTIONS),
        Route::MusicServerForm => shell
            .music_server_form
            .as_ref()
            .map(MusicServerForm::actions),
        Route::UpdateBundleForm => shell.update_form.as_ref().map(|_| 3),
        _ => None,
    }
}

pub(super) fn kinds(app: &App, rows: &[Entry]) -> Vec<wros_shell::RowKind> {
    match form_options(app) {
        Some(options) => (0..rows.len())
            .map(|i| {
                if i < options {
                    wros_shell::RowKind::Option
                } else {
                    wros_shell::RowKind::Info
                }
            })
            .collect(),
        None => rows.iter().map(Entry::kind).collect(),
    }
}

/// Rows for `actions`, in that order, labelled as the Settings submenu labels them
/// (a label can say the current state, as "Turn spellcheck off").
fn setting_rows(app: &App, actions: &[Action]) -> Vec<Entry> {
    let menu = app.setting_rows();
    actions
        .iter()
        .filter_map(|wanted| {
            menu.iter()
                .find(|(_, action)| action == wanted)
                .map(|(label, action)| Entry::new(label.clone(), Choice::Action(action.clone())))
        })
        .collect()
}

fn music_source_rows(app: &App) -> Vec<Entry> {
    let source = app.music.source;
    let label = source.label();
    match source {
        music::Source::Files => {
            let (songs, folders) = app.deck.shell.music_file_counts;
            vec![
                Entry::info(format!(
                    "Plays music in ~/Music and on USB sticks. \
                     Found {songs} songs in {folders} folders."
                )),
                Entry::new("Open the music player", Choice::Action(Action::MusicPlayer)),
            ]
        }
        music::Source::YouTubeMusic if music::ytm_helper_path().is_some() => vec![
            Entry::new("Search YouTube Music…", Choice::Action(Action::MusicPlayer)),
            Entry::new(
                match music::Config::load().youtube_play {
                    music::YoutubePlay::Stream => "Play · as it arrives",
                    music::YoutubePlay::Download => "Play · after downloading",
                },
                Choice::YoutubePlayToggle,
            ),
            Entry::info(
                "Search songs, albums and playlists. No sign-in needed. \
                 Songs start playing while they arrive; after downloading waits \
                 for the whole song, kept in a small cache.",
            ),
        ],
        music::Source::Jellyfin | music::Source::Plex => {
            let server = music::Config::load().server;
            vec![
                Entry::new(
                    format!("Set up {label} server…"),
                    Choice::Open(Route::MusicServerForm),
                ),
                Entry::info(if server.is_empty() {
                    "Not set up yet".into()
                } else {
                    format!("Server · {server}")
                }),
            ]
        }
        music::Source::Spotify if !app.wros_music => vec![
            Entry::info(
                "Controls the Spotify desktop app while it runs. Nothing to sign in to here.",
            ),
            Entry::new("Open the music player", Choice::Action(Action::MusicPlayer)),
        ],
        music::Source::Spotify => vec![Entry::info(music::SPOTIFY_UNAVAILABLE)],
        _ => vec![Entry::info(format!(
            "{label} is set up here in a later update."
        ))],
    }
}

/// The rows a Settings page shows, by label.
#[cfg(test)]
pub(crate) fn page_labels(app: &App, route: Route) -> Vec<String> {
    page_entries(app, route)
        .into_iter()
        .map(|e| e.label)
        .collect()
}

/// The menu actions a Settings page offers as rows.
#[cfg(test)]
pub(crate) fn page_actions(app: &App, route: Route) -> Vec<Action> {
    page_entries(app, route)
        .into_iter()
        .filter_map(|e| match e.choice {
            Choice::Action(action) => Some(action),
            _ => None,
        })
        .collect()
}

/// The open Settings page as a test driver sees it: its title, and each row with
/// whether it can be chosen.
#[cfg(feature = "test-driver")]
pub(crate) fn driver_page(app: &App) -> (String, Vec<(String, bool)>) {
    let route = app.deck.shell.nav.current();
    let title = breadcrumb(route).unwrap_or_else(|| "WrOS · Settings".into());
    let rows = entries(app)
        .into_iter()
        .map(|entry| {
            let choosable = !matches!(entry.choice, Choice::Information)
                && !(matches!(entry.choice, Choice::Lock) && app.deck.busy);
            (entry.label, choosable)
        })
        .collect();
    (title, rows)
}
