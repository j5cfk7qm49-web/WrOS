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

/// The first WrOS that reads the new names.
const FIRST_NEW: u32 = 22;

/// Whether the version going back to (as the recovery status names it: "wros 0.1.0~rc21",
/// "0.1.0~rc21") reads only the old names. Anything it can't make sense of is treated as older:
/// renaming back is always safe, because every newer version reads the old names too.
pub(crate) fn needs_old_names(previous: Option<&str>) -> bool {
    let Some(version) = previous.and_then(|v| v.split_whitespace().last()) else {
        return true;
    };
    let (release, rc) = match version.split_once('~') {
        Some((release, pre)) => match pre.strip_prefix("rc").and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => (release, Some(n)),
            None => return true,
        },
        None => (version, None),
    };
    let parts: Vec<u32> = match release.split('.').map(str::parse).collect() {
        Ok(parts) => parts,
        Err(_) => return true,
    };
    match (parts.as_slice(), rc) {
        ([0, 1, 0], Some(n)) => n < FIRST_NEW,
        // 0.1.0 itself, and anything after it, came after rc22.
        ([major, minor, _], _) => (*major, *minor) < (0, 1),
        _ => true,
    }
}

/// "1 project's", "3 projects'".
pub(crate) fn projects_possessive(n: usize) -> String {
    if n == 1 {
        "1 project's".into()
    } else {
        format!("{n} projects'")
    }
}

impl App {
    /// The projects on the shelf and remembered here whose files have a new name, each once.
    pub(crate) fn projects_with_new_names(&self) -> Vec<std::path::PathBuf> {
        let documents = crate::books::documents();
        let mut roots: Vec<std::path::PathBuf> = vec![self.project.root.clone()];
        roots.extend(
            grimoire_core::shelf::find_books(&documents, 3, 500)
                .into_iter()
                .map(|book| book.root),
        );
        if let Some(state) = &self.state_file {
            roots.extend(crate::books::recent(state));
        }
        let mut seen = Vec::new();
        for root in roots {
            let root = std::fs::canonicalize(&root).unwrap_or(root);
            let new_names = project_files::FileKind::ALL
                .iter()
                .any(|kind| root.join(kind.name()).symlink_metadata().is_ok());
            if new_names && !seen.contains(&root) {
                seen.push(root);
            }
        }
        seen
    }

    /// Give these projects their old file names back, after saving anything unsaved, and stop
    /// renaming them again in this session. Returns how many were renamed and why any weren't.
    pub(crate) fn rename_projects_back(
        &mut self,
        projects: &[std::path::PathBuf],
    ) -> (usize, Vec<String>) {
        self.commit_saves();
        self.names_held = true;
        let mut renamed = 0;
        let mut failed = Vec::new();
        for root in projects {
            match project_files::restore_legacy(root) {
                Ok(0) => {}
                Ok(_) => renamed += 1,
                Err(e) => failed.push(format!("{}: {e:#}", crate::books::pretty(root))),
            }
        }
        // The open project's layout guide moved: read the tree again.
        self.tree_stale = true;
        (renamed, failed)
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
