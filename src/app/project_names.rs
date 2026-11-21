//! A project's files take their rc22 names (project.toml, .grimoire/project-id,
//! Project-Format.md) when the desk opens it, and go back to the old ones before WrOS goes back
//! to a version that reads only those. See `grimoire_core::project_files`.

use super::App;
use grimoire_core::project_files;

impl App {
    /// Whether a project's files may be renamed now. Off WrOS, at once. On WrOS, only once this
    /// version has shown it starts (an update on probation can still be undone by itself at the
    /// next start, and the version it goes back to reads only the old names), never while a
    /// go-back is staged, and not after Go back renamed them back in this session.
    pub(crate) fn names_may_move(&self) -> bool {
        if self.names_held {
            return false;
        }
        if !self.deck.online_updates {
            return true;
        }
        let flow = &self.deck.shell.update_flow;
        self.deck
            .shell
            .recovery
            .as_ref()
            .is_some_and(|r| (!r.probation || flow.ready_reported) && !r.staged_update)
    }

    /// Once per project, when allowed: rename the open project's old file names, re-read its
    /// tree, and say what happened.
    pub fn tick_project_names(&mut self) {
        if self.names_checked.as_ref() == Some(&self.project.root) {
            return;
        }
        let root = self.project.root.clone();
        // Look at the disk once per project; then wait, without looking again, until renaming
        // is allowed.
        if self.names_waiting.as_ref() != Some(&root) {
            if !project_files::has_legacy(&root) {
                self.names_checked = Some(root);
                return;
            }
            self.names_waiting = Some(root.clone());
        }
        if !self.names_may_move() {
            return;
        }
        // Nothing unsaved may be written back under an old name after the rename.
        if !self.commit_saves() || self.project.dirty_count() != 0 {
            return;
        }
        let done = project_files::migrate(&root);
        self.names_checked = Some(root);
        if !done.renamed.is_empty() || !done.trashed.is_empty() {
            self.tree_stale = true;
        }
        self.msg = names_note(&done);
    }
}

/// What the status bar says after renaming.
pub(crate) fn names_note(done: &project_files::Migration) -> String {
    if let Some(why) = &done.kept_old {
        return format!("Couldn't rename this project's files ({why}); it opened as before.");
    }
    if let Some((kind, _)) = done.trashed.first() {
        return format!(
            "{} was replaced by {}; the old one is in this project's Trash.",
            kind.legacy(),
            kind.name()
        );
    }
    "This project's files now use the new names (project.toml).".into()
}
