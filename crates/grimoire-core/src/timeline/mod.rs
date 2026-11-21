//! Whole-book snapshots in the local Writing sessions repository.

use crate::sessions::{self, Store, git};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

mod bundles;
pub use bundles::{Bundle, is_damaged, machine_name, mark_uploaded, next_bundle, rebuild};

/// Book-relative globs for generated files and machine-local or unsettled state.
pub const EXCLUDED: &[&str] = &[
    ".grimoire/history/**",
    ".grimoire/recovery/**",
    ".grimoire/retained-history/**",
    ".grimoire/sync.json",
    "exports/**",
    "**/.wros-*/**",
    "**/* (from *",
    // An in-book `.git` that `enable` moved out is parked in the trash: a whole repository,
    // never book content.
    ".grimoire/trash/.git-moved-out-*/**",
];

pub enum Entry {
    Save,
    ChangedOnDrive(Vec<String>),
    BeforeRestoring(String),
    BeforeReplacing,
    BroughtIn,
}

/// Set up the local timeline, keeping any history the book already has.
pub fn enable(root: &Path) -> Result<()> {
    if !sessions::git_available() {
        bail!("git isn't installed, so the book's timeline isn't available");
    }
    if !root.is_dir() {
        bail!("{} isn't a folder", root.display());
    }
    let root = &std::path::absolute(root)?;
    if sessions::store(root) == Store::InBook {
        sessions::move_history_out(root)?;
    }
    let repo = match sessions::store(root) {
        Store::Outside(repo) => repo,
        _ => {
            let taken = sessions::outside_dir(root).is_some_and(|dir| dir.exists());
            sessions::assign_id(root, taken)?;
            let repo = sessions::outside_dir(root).context("giving the book an id")?;
            init_repository(root, &repo)?;
            repo
        }
    };

    configure_repository(root, &repo)?;
    if sessions::head_commit(root)?.is_none() {
        stage(root, &repo)?;
        sessions::commit(root, "Timeline begins")?;
    }
    Ok(())
}

fn init_repository(root: &Path, repo: &Path) -> Result<()> {
    if let Some(parent) = repo.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut args = Vec::new();
    if sessions::config(root, "init.defaultBranch")?.is_none() {
        args.extend(["-c", "init.defaultBranch=main"]);
    }
    args.extend(["init", "--bare", "-q"]);
    git(root, args).env("GIT_DIR", repo).stdout()?;
    let here = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    fs::write(
        repo.join("grimoire-book"),
        here.to_string_lossy().as_bytes(),
    )?;
    Ok(())
}

fn configure_repository(root: &Path, repo: &Path) -> Result<()> {
    git(root, ["config", "--local", "core.fsync", "committed"]).stdout()?;
    git(root, ["config", "--local", "core.fsyncMethod", "fsync"]).stdout()?;
    let info = repo.join("info");
    fs::create_dir_all(&info)?;
    fs::write(info.join("exclude"), format!("{}\n", EXCLUDED.join("\n")))?;
    Ok(())
}

/// Whether the book has a usable, claimed repository outside its work tree.
pub fn is_enabled(root: &Path) -> bool {
    std::path::absolute(root).is_ok_and(|root| {
        matches!(sessions::store(&root), Store::Outside(_)) && sessions::is_enabled(&root)
    })
}

/// Record the current book and return the short hash if its tree changed.
pub fn record(root: &Path, entry: &Entry) -> Result<Option<String>> {
    let root = &std::path::absolute(root)?;
    let Store::Outside(repo) = sessions::store(root) else {
        bail!("the timeline isn't enabled for {}", root.display());
    };
    stage(root, &repo)?;
    let diff = git(
        root,
        [
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--word-diff=porcelain",
            "-M",
        ],
    )
    .stdout()?;
    if diff.is_empty() {
        return Ok(None);
    }
    let words = words_changed(&String::from_utf8_lossy(&diff));
    sessions::commit(root, &label(entry, words))?;
    Ok(Some(git(root, ["rev-parse", "--short", "HEAD"]).text()?))
}

pub fn label(entry: &Entry, words_changed: usize) -> String {
    match entry {
        Entry::Save => match words_changed {
            0..=10 => "minor fix",
            11..=150 => "small change",
            151..=1500 => "change",
            _ => "big change",
        }
        .into(),
        Entry::ChangedOnDrive(files) => match files.as_slice() {
            [] => "Changed on Drive".into(),
            [a, b, c, rest @ ..] if !rest.is_empty() => {
                format!("Changed on Drive: {a}, {b}, {c} and {} more", rest.len())
            }
            _ => format!("Changed on Drive: {}", files.join(", ")),
        },
        Entry::BeforeRestoring(title) => format!("Before restoring {title}"),
        Entry::BeforeReplacing => "Before replacing across the book".into(),
        Entry::BroughtIn => "Brought in from Drive".into(),
    }
}

/// Mark the entries since the previous session and return the new tag name.
pub fn mark_session(root: &Path, now: DateTime<Local>) -> Result<Option<String>> {
    let root = &std::path::absolute(root)?;
    let head = timeline_head(root)?;
    let tags = session_tags(root)?;
    let labels = session_labels(&tags);
    if labels.contains_key(head.as_str()) {
        return Ok(None);
    }
    let commits = if labels.is_empty() {
        String::new()
    } else {
        timeline_commits(root, &head)?
    };
    // Use history order even when the clock repeats or moves backwards.
    let base = commits.lines().find(|hash| labels.contains_key(*hash));

    let stem = format!("session/{}", now.format("%Y-%m-%d-%H%M"));
    let mut name = stem.clone();
    let mut suffix = 2;
    while tags.iter().any(|tag| tag.name == name) {
        name = format!("{stem}-{suffix}");
        suffix += 1;
    }
    let tagger_date = format!("{} {}", now.timestamp(), now.format("%z"));
    let diary = sessions::range_label(root, base, &head, now)?;
    sessions::git_with_identity(
        root,
        &["tag", "-a", "--no-sign", "-m", &diary, "--", &name, &head],
    )?
    .env("GIT_COMMITTER_DATE", tagger_date)
    .stdout()?;
    Ok(Some(name))
}

#[derive(Debug, Clone)]
pub struct SceneVersion {
    pub hash: String,
    pub when: DateTime<Local>,
    pub label: String,
    /// The diary label when this is the scene's newest version in a marked session.
    pub session: Option<String>,
    pub text: String,
}

/// The newest recorded versions of a book-relative scene (or an absolute path inside the book).
pub fn scene_versions(root: &Path, scene: &Path, limit: usize) -> Result<Vec<SceneVersion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let root = &std::path::absolute(root)?;
    let head = timeline_head(root)?;
    let rel = sessions::git_path(scene.strip_prefix(root).unwrap_or(scene))?;
    let max_count = format!("--max-count={}", limit.min(i32::MAX as usize));
    let raw = git(
        root,
        [
            "log",
            "--follow",
            "-M",
            "-z",
            "--format=%H%x1f%at%x1f%s",
            "--name-status",
            "--diff-filter=d",
            "--topo-order",
            "--no-show-signature",
            "--no-notes",
            "--no-color",
            &max_count,
            &head,
            "--",
            &rel,
        ],
    )
    .env("GIT_LITERAL_PATHSPECS", "1")
    .stdout()?;

    // Each header is followed by one name-status entry. For a rename the second
    // path belongs to this commit; --follow already uses the old path farther back.
    let log = String::from_utf8_lossy(&raw);
    let mut fields = log.split_terminator('\0');
    let mut versions = Vec::new();
    while let Some(header) = fields.next() {
        let mut header = header.splitn(3, '\x1f');
        let hash = header.next().context("missing scene version hash")?;
        let timestamp = header
            .next()
            .context("missing scene version date")?
            .parse()?;
        let when = DateTime::from_timestamp(timestamp, 0)
            .context("invalid scene version date")?
            .with_timezone(&Local);
        let label = header.next().context("missing scene version label")?;
        let status = fields.next().context("missing scene version status")?;
        let mut path = fields.next().context("missing scene version path")?;
        if status.trim_start_matches('\n').starts_with(['R', 'C']) {
            path = fields.next().context("missing renamed scene path")?;
        }
        let text = git(root, ["show", &format!("{hash}:{path}")]).stdout()?;
        versions.push(SceneVersion {
            hash: hash.into(),
            when,
            label: label.into(),
            session: None,
            text: String::from_utf8_lossy(&text).into_owned(),
        });
    }
    add_session_labels(root, &head, &mut versions)?;
    Ok(versions)
}

fn timeline_head(root: &Path) -> Result<String> {
    if !matches!(sessions::store(root), Store::Outside(_)) {
        bail!("the timeline isn't enabled for {}", root.display());
    }
    git(root, ["rev-parse", "--verify", "HEAD^{commit}"]).text()
}

struct SessionTag {
    name: String,
    commit: String,
    label: String,
}

fn session_tags(root: &Path) -> Result<Vec<SessionTag>> {
    let raw = git(
        root,
        [
            "for-each-ref",
            "--sort=-creatordate",
            "--format=%(refname:strip=2)%00%(*objectname)%00%(contents:subject)",
            "refs/tags/session/",
        ],
    )
    .stdout()?;
    Ok(String::from_utf8_lossy(&raw)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\0');
            Some(SessionTag {
                name: fields.next()?.into(),
                commit: fields.next()?.into(),
                label: fields.next()?.into(),
            })
        })
        .collect())
}

fn session_labels(tags: &[SessionTag]) -> HashMap<&str, &str> {
    let mut labels = HashMap::new();
    for tag in tags.iter().filter(|tag| !tag.commit.is_empty()) {
        labels
            .entry(tag.commit.as_str())
            .or_insert(tag.label.as_str());
    }
    labels
}

fn timeline_commits(root: &Path, head: &str) -> Result<String> {
    git(root, ["rev-list", "--topo-order", head]).text()
}

fn add_session_labels(root: &Path, head: &str, versions: &mut [SceneVersion]) -> Result<()> {
    let mut versions = versions.iter_mut();
    let Some(mut version) = versions.next() else {
        return Ok(());
    };
    let tags = session_tags(root)?;
    let labels = session_labels(&tags);
    if labels.is_empty() {
        return Ok(());
    }

    let mut pending = None;
    for hash in timeline_commits(root, head)?.lines() {
        // A tag can sit on an unrelated edit. Carry its label back to the first scene
        // version in that session, or replace it at the next tag if the scene didn't change.
        if let Some(&label) = labels.get(hash) {
            pending = Some(label);
        }
        if version.hash == hash {
            version.session = pending.take().map(String::from);
            let Some(next) = versions.next() else { break };
            version = next;
        }
    }
    Ok(())
}

/// An index.lock older than this was left by a git call that never finished: every git call here
/// has a 30-second deadline and the app records one book from one worker at a time.
const STALE_LOCK: std::time::Duration = std::time::Duration::from_secs(120);

fn stage(root: &Path, repo: &Path) -> Result<()> {
    let index = repo.join("index");
    clear_stale_lock(&repo.join("index.lock"));
    // Older sessions may already track some excluded paths. Drop only their index entries.
    let remove = ["rm", "--cached", "-r", "-f", "--ignore-unmatch", "-q", "--"]
        .into_iter()
        .map(String::from)
        .chain(EXCLUDED.iter().map(|pattern| format!(":(glob){pattern}")));
    git(root, remove).index(Some(&index)).stdout()?;

    let add = ["add", "--all", "--force", "--", "."]
        .into_iter()
        .map(String::from)
        .chain(
            EXCLUDED
                .iter()
                .map(|pattern| format!(":(exclude,glob){pattern}")),
        );
    git(root, add).index(Some(&index)).stdout()?;
    Ok(())
}

/// Removes a lock git left behind (killed at its deadline, or a power cut), which would otherwise
/// make every later record fail. A fresh lock is left alone.
fn clear_stale_lock(lock: &Path) {
    let stale = fs::metadata(lock)
        .and_then(|meta| meta.modified())
        .is_ok_and(|when| when.elapsed().is_ok_and(|age| age > STALE_LOCK));
    if stale {
        let _ = fs::remove_file(lock);
    }
}

fn words_changed(diff: &str) -> usize {
    // The --- and +++ file headers precede the first hunk of each file.
    diff.split("\ndiff --git ")
        .flat_map(|file| file.lines().skip_while(|line| !line.starts_with("@@ ")))
        .filter_map(|line| line.strip_prefix('+').or_else(|| line.strip_prefix('-')))
        .map(sessions::body_words)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::{cleanup, temp_dir, write};
    use chrono::TimeZone;
    use std::path::PathBuf;

    const SCENE: &str = "manuscript/02-Act-Two/01-Chapter/01-Scene.md";

    fn session_time() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 17, 19, 15, 0).unwrap()
    }

    fn book(tag: &str) -> Option<PathBuf> {
        if !sessions::git_available() {
            eprintln!("skipping {tag}: git isn't installed");
            return None;
        }
        Some(temp_dir(tag))
    }

    fn committed_files(root: &Path) -> Vec<String> {
        git(root, ["ls-tree", "-r", "--name-only", "HEAD"])
            .text()
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    fn subject(root: &Path) -> String {
        git(root, ["log", "-1", "--format=%s"]).text().unwrap()
    }

    fn clean_book(root: &Path) {
        cleanup(&[root, &sessions::outside_dir(root).unwrap()]);
    }

    #[test]
    fn records_the_whole_book_except_the_excluded_set() {
        let Some(root) = book("timeline-whole-book") else {
            return;
        };
        let recorded = [
            ".grimoire/project-id",
            ".grimoire/resume.md",
            ".grimoire/trash/cut.md",
            ".grimoire/writingdeck/journal.json",
            "annotations.json",
            "chapters/01.md",
            "notes/x.md",
            "novel.toml",
        ];
        for path in recorded.into_iter().chain([
            ".grimoire/history/a/1.md",
            ".grimoire/recovery/r",
            ".grimoire/retained-history/h",
            ".grimoire/sync.json",
            "exports/book.pdf",
            ".wros-stage-1/x",
            "chapters/.wros-stage-2/nested/x",
            "chapters/01 (from Drive, 2026-11-16).md",
            "01 (from Drive, 2026-11-16).md",
        ]) {
            write(&root, path, "test\n");
        }
        assert!(!is_enabled(&root));
        enable(&root).unwrap();
        assert!(is_enabled(&root));
        assert!(!root.join(".git").exists());
        assert!(!root.join(".gitignore").exists());
        assert_eq!(subject(&root), "Timeline begins");
        assert_eq!(committed_files(&root), recorded);
        clean_book(&root);
    }

    #[test]
    fn hostile_gitignore_cannot_hide_files() {
        let Some(root) = book("timeline-gitignore") else {
            return;
        };
        write(&root, ".gitignore", "*\n");
        write(&root, "chapters/01.md", "A beginning.\n");
        write(&root, ".grimoire/resume.md", "Chapter One\n");
        enable(&root).unwrap();
        assert_eq!(fs::read_to_string(root.join(".gitignore")).unwrap(), "*\n");
        assert_eq!(
            committed_files(&root),
            [
                ".gitignore",
                ".grimoire/project-id",
                ".grimoire/resume.md",
                "chapters/01.md"
            ]
        );

        write(&root, ".gitignore", "*.md\n.grimoire/\n");
        write(&root, "chapters/02.md", "A continuation.\n");
        write(&root, ".grimoire/writingdeck/journal.json", "{}\n");
        write(&root, ".grimoire/history/a/1.md", "Old history\n");
        assert!(record(&root, &Entry::Save).unwrap().is_some());
        assert_eq!(
            committed_files(&root),
            [
                ".gitignore",
                ".grimoire/project-id",
                ".grimoire/resume.md",
                ".grimoire/writingdeck/journal.json",
                "chapters/01.md",
                "chapters/02.md",
            ]
        );
        clean_book(&root);
    }

    #[test]
    fn nothing_changed_records_nothing() {
        let Some(root) = book("timeline-unchanged") else {
            return;
        };
        enable(&root).unwrap();
        write(&root, "chapters/01.md", "First draft.\n");
        let hash = record(&root, &Entry::Save).unwrap().unwrap();
        write(&root, "exports/book.pdf", "Unrecorded export\n");
        assert_eq!(record(&root, &Entry::Save).unwrap(), None);
        assert_eq!(
            git(&root, ["rev-parse", "--short", "HEAD"]).text().unwrap(),
            hash
        );
        clean_book(&root);
    }

    #[test]
    fn enable_writes_no_gitignore_and_keeps_an_existing_sessions_history() {
        let Some(root) = book("timeline-sessions") else {
            return;
        };
        write(&root, "chapters/01.md", "First draft.\n");
        let conflict = "chapters/01 (from Drive, 2026-11-16).md";
        write(&root, conflict, "Parked copy.\n");
        sessions::enable(&root).unwrap();
        let old_repo = sessions::outside_dir(&root).unwrap();
        let old_head = git(&root, ["rev-parse", "HEAD"]).text().unwrap();
        assert!(committed_files(&root).contains(&conflict.to_string()));
        fs::remove_file(root.join(".gitignore")).unwrap();

        enable(&root).unwrap();
        assert_eq!(sessions::outside_dir(&root).unwrap(), old_repo);
        assert_eq!(git(&root, ["rev-parse", "HEAD"]).text().unwrap(), old_head);
        assert!(!root.join(".gitignore").exists());

        record(&root, &Entry::Save).unwrap().unwrap();
        assert_eq!(git(&root, ["rev-parse", "HEAD^"]).text().unwrap(), old_head);
        assert_eq!(
            committed_files(&root),
            [".grimoire/project-id", "chapters/01.md"]
        );
        assert_eq!(
            fs::read_to_string(root.join(conflict)).unwrap(),
            "Parked copy.\n"
        );
        clean_book(&root);
    }

    #[test]
    fn enable_moves_an_in_book_git_out_first() {
        let Some(root) = book("timeline-move-out") else {
            return;
        };
        git(&root, ["init", "-q"]).stdout().unwrap();
        write(&root, "chapters/01.md", "First draft.\n");
        sessions::enable(&root).unwrap();
        let old_head = git(&root, ["rev-parse", "HEAD"]).text().unwrap();
        let ignore = fs::read(root.join(".gitignore")).unwrap();
        assert!(!is_enabled(&root));

        enable(&root).unwrap();
        assert!(is_enabled(&root));
        assert!(!root.join(".git").exists());
        assert_eq!(git(&root, ["rev-parse", "HEAD"]).text().unwrap(), old_head);
        assert_eq!(fs::read(root.join(".gitignore")).unwrap(), ignore);
        // The parked old repository is never recorded as book content.
        write(&root, "chapters/01.md", "Second draft.\n");
        record(&root, &Entry::Save).unwrap().unwrap();
        let tree = git(&root, ["ls-tree", "-r", "--name-only", "HEAD"])
            .text()
            .unwrap();
        assert!(!tree.contains(".git-moved-out-"), "{tree}");
        clean_book(&root);
    }

    #[test]
    fn a_stale_index_lock_does_not_stop_recording() {
        let Some(root) = book("timeline-stale-lock") else {
            return;
        };
        write(&root, "chapters/01.md", "First draft.\n");
        enable(&root).unwrap();
        let lock = sessions::outside_dir(&root).unwrap().join("index.lock");
        // A git call killed mid-commit leaves its lock behind.
        let file = fs::File::create(&lock).unwrap();
        file.set_modified(std::time::SystemTime::now() - STALE_LOCK * 2)
            .unwrap();
        drop(file);
        write(&root, "chapters/01.md", "Second draft.\n");
        assert!(record(&root, &Entry::Save).unwrap().is_some());
        // A lock that may belong to a git call still running is left alone.
        fs::File::create(&lock).unwrap();
        write(&root, "chapters/01.md", "Third draft.\n");
        assert!(record(&root, &Entry::Save).is_err());
        assert!(lock.exists());
        fs::remove_file(&lock).unwrap();
        clean_book(&root);
    }

    #[test]
    fn fsync_is_configured() {
        let Some(root) = book("timeline-fsync") else {
            return;
        };
        enable(&root).unwrap();
        assert_eq!(
            git(&root, ["config", "--local", "--get", "core.fsync"])
                .text()
                .unwrap(),
            "committed"
        );
        assert_eq!(
            git(&root, ["config", "--local", "--get", "core.fsyncMethod"])
                .text()
                .unwrap(),
            "fsync"
        );
        let excludes =
            fs::read_to_string(sessions::outside_dir(&root).unwrap().join("info/exclude")).unwrap();
        assert_eq!(excludes.lines().collect::<Vec<_>>(), EXCLUDED);
        clean_book(&root);
    }

    #[test]
    fn labels_follow_the_words_changed() {
        for (words, want) in [
            (3, "minor fix"),
            (10, "minor fix"),
            (11, "small change"),
            (40, "small change"),
            (150, "small change"),
            (151, "change"),
            (400, "change"),
            (1500, "change"),
            (1501, "big change"),
            (4000, "big change"),
        ] {
            assert_eq!(label(&Entry::Save, words), want);
        }
        assert_eq!(
            label(
                &Entry::ChangedOnDrive(vec!["a.md".into(), "b.md".into()]),
                0
            ),
            "Changed on Drive: a.md, b.md"
        );
        let files = ["a.md", "b.md", "c.md", "d.md", "e.md"];
        assert_eq!(
            label(&Entry::ChangedOnDrive(files.map(String::from).to_vec()), 0),
            "Changed on Drive: a.md, b.md, c.md and 2 more"
        );
        assert_eq!(label(&Entry::ChangedOnDrive(vec![]), 0), "Changed on Drive");
        assert_eq!(
            label(&Entry::BeforeRestoring("Chapter One".into()), 0),
            "Before restoring Chapter One"
        );
        assert_eq!(
            label(&Entry::BeforeReplacing, 0),
            "Before replacing across the book"
        );
        assert_eq!(label(&Entry::BroughtIn, 0), "Brought in from Drive");
    }

    #[test]
    fn save_labels_count_added_and_removed_words_and_follow_renames() {
        let Some(root) = book("timeline-word-diff") else {
            return;
        };
        write(&root, "chapters/01.md", "one two three four five six\n");
        enable(&root).unwrap();
        write(
            &root,
            "chapters/01.md",
            "seven eight nine ten eleven twelve\n",
        );
        record(&root, &Entry::Save).unwrap().unwrap();
        assert_eq!(subject(&root), "small change");

        fs::rename(root.join("chapters/01.md"), root.join("chapters/02.md")).unwrap();
        record(&root, &Entry::Save).unwrap().unwrap();
        assert_eq!(subject(&root), "minor fix");
        assert_eq!(
            committed_files(&root),
            [".grimoire/project-id", "chapters/02.md"]
        );
        clean_book(&root);
    }

    #[test]
    fn marks_a_session_once() {
        let Some(root) = book("timeline-session-once") else {
            return;
        };
        enable(&root).unwrap();
        let tag = mark_session(&root, session_time()).unwrap().unwrap();
        assert_eq!(tag, "session/2026-11-17-1915");
        let tag_ref = format!("refs/tags/{tag}");
        assert_eq!(
            git(&root, ["cat-file", "-t", &tag_ref]).text().unwrap(),
            "tag"
        );
        assert_eq!(
            git(&root, ["rev-parse", &format!("{tag_ref}^{{commit}}")])
                .text()
                .unwrap(),
            git(&root, ["rev-parse", "HEAD"]).text().unwrap()
        );
        assert_eq!(mark_session(&root, session_time()).unwrap(), None);
        clean_book(&root);
    }

    #[test]
    fn session_label_reads_like_the_diary() {
        let Some(root) = book("timeline-session-diary") else {
            return;
        };
        write(
            &root,
            SCENE,
            "---\ntitle: Beginnings\n---\nOne two three.\n",
        );
        enable(&root).unwrap();
        let first = mark_session(&root, session_time()).unwrap().unwrap();
        assert_eq!(
            tag_label(&root, &first),
            "Tuesday evening · Act Two · 3 words"
        );

        write(&root, SCENE, "A brief detour.\n");
        record(&root, &Entry::Save).unwrap().unwrap();
        write(
            &root,
            SCENE,
            "---\ntitle: Revised title\n---\nOne two three four five.\n",
        );
        record(&root, &Entry::Save).unwrap().unwrap();
        let second = mark_session(&root, session_time()).unwrap().unwrap();
        assert_eq!(
            tag_label(&root, &second),
            "Tuesday evening · Act Two · 2 words"
        );
        clean_book(&root);
    }

    fn tag_label(root: &Path, tag: &str) -> String {
        git(
            root,
            [
                "for-each-ref",
                "--format=%(contents:subject)",
                &format!("refs/tags/{tag}"),
            ],
        )
        .text()
        .unwrap()
    }

    #[test]
    fn same_minute_gets_a_suffix() {
        let Some(root) = book("timeline-session-suffix") else {
            return;
        };
        enable(&root).unwrap();
        mark_session(&root, session_time()).unwrap().unwrap();
        for suffix in [2, 3] {
            write(&root, SCENE, &format!("Draft {suffix}\n"));
            record(&root, &Entry::Save).unwrap().unwrap();
            assert_eq!(
                mark_session(&root, session_time()).unwrap(),
                Some(format!("session/2026-11-17-1915-{suffix}"))
            );
        }
        assert_eq!(mark_session(&root, session_time()).unwrap(), None);
        clean_book(&root);
    }

    #[test]
    fn scene_versions_newest_first_with_labels() {
        let Some(root) = book("timeline-scene-versions") else {
            return;
        };
        write(&root, SCENE, "First draft.\n");
        enable(&root).unwrap();
        write(&root, SCENE, "\nRevised draft.\n\n");
        let hash = record(&root, &Entry::ChangedOnDrive(vec![SCENE.into()]))
            .unwrap()
            .unwrap();
        let timestamp = git(&root, ["log", "-1", "--format=%at"])
            .text()
            .unwrap()
            .parse::<i64>()
            .unwrap();
        write(&root, "notes/x.md", "Unrelated edit\n");
        record(&root, &Entry::Save).unwrap().unwrap();

        let versions = scene_versions(&root, &root.join(SCENE), 10).unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions[0].hash.starts_with(hash.as_str()));
        assert_eq!(versions[0].when.timestamp(), timestamp);
        assert_eq!(versions[0].label, format!("Changed on Drive: {SCENE}"));
        assert_eq!(versions[0].text, "\nRevised draft.\n\n");
        assert_eq!(versions[1].label, "Timeline begins");
        assert_eq!(versions[1].text, "First draft.\n");
        assert_eq!(scene_versions(&root, Path::new(SCENE), 1).unwrap().len(), 1);
        assert!(
            scene_versions(&root, Path::new(SCENE), 0)
                .unwrap()
                .is_empty()
        );
        clean_book(&root);
    }

    #[test]
    fn scene_versions_follow_a_rename() {
        let Some(root) = book("timeline-scene-rename") else {
            return;
        };
        let old: String = (1..=20)
            .map(|i| format!("Line {i} of the scene.\n"))
            .collect();
        write(&root, SCENE, &old);
        enable(&root).unwrap();
        let new = root.join("manuscript/02-Act-Two/01-Chapter/02-Scène [revised].md");
        fs::rename(root.join(SCENE), &new).unwrap();
        let revised = format!("{old}One added line.\n");
        fs::write(&new, &revised).unwrap();
        record(&root, &Entry::Save).unwrap().unwrap();

        let versions = scene_versions(&root, &new, 10).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].text, revised);
        assert_eq!(versions[1].text, old);
        clean_book(&root);
    }

    #[test]
    fn scene_versions_skip_deletions_without_spending_the_limit() {
        let Some(root) = book("timeline-scene-deletion") else {
            return;
        };
        write(&root, SCENE, "First draft.\n");
        enable(&root).unwrap();
        fs::remove_file(root.join(SCENE)).unwrap();
        record(&root, &Entry::Save).unwrap().unwrap();

        let versions = scene_versions(&root, Path::new(SCENE), 1).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].text, "First draft.\n");
        clean_book(&root);
    }

    #[test]
    fn session_divider_on_the_last_entry_of_a_session() {
        let Some(root) = book("timeline-session-divider") else {
            return;
        };
        write(&root, SCENE, "First draft.\n");
        enable(&root).unwrap();
        write(&root, SCENE, "Revised draft.\n");
        record(&root, &Entry::Save).unwrap().unwrap();
        write(&root, "notes/x.md", "End of the first session\n");
        record(&root, &Entry::Save).unwrap().unwrap();
        let first = mark_session(&root, session_time()).unwrap().unwrap();

        write(
            &root,
            "notes/x.md",
            "The second session only changed notes\n",
        );
        record(&root, &Entry::Save).unwrap().unwrap();
        mark_session(&root, session_time()).unwrap().unwrap();
        write(&root, SCENE, "An edit in the open session.\n");
        record(&root, &Entry::Save).unwrap().unwrap();

        let versions = scene_versions(&root, Path::new(SCENE), 10).unwrap();
        let diary = tag_label(&root, &first);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].session, None);
        assert_eq!(versions[1].session.as_deref(), Some(diary.as_str()));
        assert_eq!(versions[2].session, None);
        clean_book(&root);
    }
}
