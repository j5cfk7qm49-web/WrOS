//! Project-local presentation references, never another manuscript text store.
//! Metadata transactions are guarded and rollback in memory on write failure.
use crate::{
    sync::{Fingerprint, SaveOutcome},
    workspaces::relative,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Always,
    #[default]
    Novel,
    Chapter,
    Scene,
    Dashboard,
    Hidden,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Life {
    #[default]
    Active,
    Archived,
    Trashed,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    #[default]
    Neutral,
    Amber,
    Green,
    Blue,
    Rose,
}
impl Color {
    pub fn next(self) -> Self {
        match self {
            Self::Neutral => Self::Amber,
            Self::Amber => Self::Green,
            Self::Green => Self::Blue,
            Self::Blue => Self::Rose,
            Self::Rose => Self::Neutral,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Anchor {
    Heading { text: String },
    Quote { text: String },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Free,
    Anchored,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub id: u64,
    pub source: PathBuf,
    pub scope: Scope,
    pub context: Option<PathBuf>,
    pub category: String,
    pub color: Color,
    pub life: Life,
    pub collapsed: bool,
    pub height: u16,
    pub anchor: Option<Anchor>,
    pub mode: Mode,
    pub scroll: usize,
}
impl Pin {
    pub fn visible(
        &self,
        document: Option<&Path>,
        chapter: Option<&Path>,
        dashboard: bool,
    ) -> bool {
        self.life == Life::Active
            && match self.scope {
                Scope::Always => true,
                Scope::Novel => !dashboard,
                Scope::Dashboard => dashboard,
                Scope::Hidden => false,
                Scope::Scene => !dashboard && self.context.as_deref() == document,
                Scope::Chapter => !dashboard && self.context.as_deref() == chapter,
            }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub version: u32,
    pub next_id: u64,
    pub pins: Vec<Pin>,
    pub selected: Option<u64>,
    pub sidebar_visible: bool,
    pub sidebar_width: u16,
    pub sidebar_scroll: usize,
    pub board_scroll: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            pins: vec![],
            selected: None,
            sidebar_visible: false,
            sidebar_width: 30,
            sidebar_scroll: 0,
            board_scroll: 0,
        }
    }
}
fn clean(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}
pub fn source_relative(path: &Path) -> Result<()> {
    relative(path)?;
    if path.extension().and_then(|p| p.to_str()) != Some("md")
        || path
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some(".grimoire" | ".git")))
        || path.to_str().is_none_or(|s| !clean(s, 4096))
    {
        bail!("pin source must be a project Markdown document, not internal history/recovery");
    }
    Ok(())
}
impl State {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "unsupported pins version {}; original preserved",
                self.version
            );
        }
        if self.pins.len() > 256
            || !(24..=60).contains(&self.sidebar_width)
            || self.sidebar_scroll > 256
            || self.board_scroll > 256
        {
            bail!("invalid pin count/sidebar geometry");
        }
        let mut ids = BTreeSet::new();
        for p in &self.pins {
            source_relative(&p.source)?;
            if p.id == 0
                || p.id >= self.next_id
                || !ids.insert(p.id)
                || !clean(&p.category, 40)
                || !(4..=20).contains(&p.height)
                || p.scroll > 1_000_000
            {
                bail!("invalid pin ID, category, height or scroll");
            }
            if let Some(c) = &p.context {
                relative(c)?;
            }
            if matches!(p.scope, Scope::Chapter | Scope::Scene) && p.context.is_none() {
                bail!("scene/chapter pin needs an explicit context");
            }
            if let Some(a) = &p.anchor {
                match a {
                    Anchor::Heading { text } if !text.trim().is_empty() && clean(text, 256) => {}
                    Anchor::Quote { text }
                        if !text.trim().is_empty()
                            && text.chars().count() <= 512
                            && !text
                                .chars()
                                .any(|c| c.is_control() && c != '\n' && c != '\t') => {}
                    _ => bail!("invalid semantic pin anchor"),
                }
            }
            if p.mode == Mode::Anchored && p.anchor.is_none() {
                bail!("anchored pin needs an anchor");
            }
        }
        if self.next_id == 0 || self.selected.is_some_and(|id| !ids.contains(&id)) {
            bail!("invalid selected pin or next ID");
        }
        Ok(())
    }
    pub fn add(&mut self, source: PathBuf) -> Result<u64> {
        source_relative(&source)?;
        if self.pins.len() >= 256 {
            bail!("pin limit reached (256)");
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).context("pin ID space exhausted")?;
        self.pins.push(Pin {
            id,
            source,
            scope: Scope::Novel,
            context: None,
            category: String::new(),
            color: Color::Neutral,
            life: Life::Active,
            collapsed: false,
            height: 9,
            anchor: None,
            mode: Mode::Free,
            scroll: 0,
        });
        self.selected = Some(id);
        Ok(id)
    }
    pub fn pin_mut(&mut self, id: u64) -> Result<&mut Pin> {
        self.pins
            .iter_mut()
            .find(|p| p.id == id)
            .context("pin no longer exists")
    }
    /// Recoverable card lifecycle only. Never moves/deletes the source file.
    pub fn set_life(&mut self, id: u64, life: Life) -> Result<()> {
        self.pin_mut(id)?.life = life;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SourceIdentity {
    canonical: PathBuf,
    file_id: Option<(u64, u64)>,
}
impl SourceIdentity {
    pub fn same_file(&self, other: &Self) -> bool {
        self.canonical == other.canonical || self.file_id.is_some() && self.file_id == other.file_id
    }
}
pub fn identify(root: &Path, source: &Path) -> Result<SourceIdentity> {
    source_relative(source)?;
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(root.join(source))
        .context("source missing or unreadable; reference retained")?;
    if !path.starts_with(&root) {
        bail!("source resolves outside the project; not displayed");
    }
    source_relative(path.strip_prefix(&root)?)?;
    let meta = fs::metadata(&path)?;
    if !meta.is_file() {
        bail!("source is not a regular file");
    }
    #[cfg(unix)]
    let file_id = {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    };
    #[cfg(not(unix))]
    let file_id = None;
    Ok(SourceIdentity {
        canonical: path,
        file_id,
    })
}

pub struct Store {
    pub state: State,
    root: PathBuf,
    seen: Option<Fingerprint>,
}
impl Store {
    pub fn path(root: &Path) -> PathBuf {
        root.join(".grimoire/writingdeck/pins.json")
    }
    /// Verify existing ancestors BEFORE creating directories, including a
    /// malicious .grimoire symlink. No metadata write may escape this project.
    fn safe_path(root: &Path) -> Result<PathBuf> {
        let root = fs::canonicalize(root)?;
        let path = Self::path(&root);
        for ancestor in path.ancestors().take_while(|p| *p != root) {
            match fs::symlink_metadata(ancestor) {
                Ok(_) => {
                    if !fs::canonicalize(ancestor)?.starts_with(&root) {
                        bail!("pins metadata resolves outside project; preserved");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(path)
    }
    pub fn load(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root)?;
        let path = Self::safe_path(&root)?;
        let (state, seen) = match fs::File::open(&path) {
            Ok(file) => {
                use std::io::Read;
                let mut text = String::new();
                file.take(1_048_577).read_to_string(&mut text)?;
                if text.len() > 1_048_576 {
                    bail!("pins metadata exceeds 1 MiB; original preserved");
                }
                let state: State = serde_json::from_str(&text)
                    .context("invalid/unknown pins JSON; original preserved")?;
                state.validate()?;
                (state, Some(Fingerprint::of(&text)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (State::default(), None),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { state, root, seen })
    }
    pub fn update<T>(&mut self, edit: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut next = self.state.clone();
        let result = edit(&mut next)?;
        next.validate()?;
        let text = serde_json::to_string_pretty(&next)? + "\n";
        if text.len() > 1_048_576 {
            bail!("pins metadata exceeds size limit");
        }
        let path = Self::safe_path(&self.root)?;
        fs::create_dir_all(path.parent().unwrap())?;
        match crate::sync::save_guarded(&path, &text, self.seen)? {
            SaveOutcome::Written => {
                self.seen = Some(Fingerprint::of(&text));
                self.state = next;
                Ok(result)
            }
            SaveOutcome::Conflict { .. } => {
                bail!("pins changed externally; original not overwritten; reload the board")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Free,
    Found(usize),
    Missing,
    Ambiguous,
}
pub fn headings(body: &str) -> Vec<String> {
    heading_lines(body).map(|(_, title)| title).collect()
}
fn heading_lines(body: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    let mut fence: Option<(char, usize)> = None;
    body.lines().enumerate().filter_map(move |(row, line)| {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next();
        let run = marker.map_or(0, |c| trimmed.chars().take_while(|&v| v == c).count());
        if let Some((c, n)) = fence {
            if indent <= 3 && marker == Some(c) && run >= n && trimmed[run..].trim().is_empty() {
                fence = None;
            }
            return None;
        }
        if indent <= 3 && matches!(marker, Some('`' | '~')) && run >= 3 {
            fence = Some((marker.unwrap(), run));
            return None;
        }
        heading(line).map(|title| (row, title))
    })
}
fn heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 || trimmed.starts_with('\t') {
        return None;
    }
    let n = trimmed.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&n) || !matches!(trimmed.chars().nth(n), Some(' ' | '\t')) {
        return None;
    }
    let title = trimmed[n..].trim_matches([' ', '\t']);
    let without_hashes = title.trim_end_matches('#');
    let title = if without_hashes.ends_with([' ', '\t']) {
        without_hashes.trim_end_matches([' ', '\t'])
    } else {
        title
    };
    (!title.is_empty()).then(|| title.to_string())
}
pub fn resolve(body: &str, anchor: Option<&Anchor>) -> Resolution {
    match anchor {
        None => Resolution::Free,
        Some(Anchor::Heading { text }) => {
            let mut found =
                heading_lines(body).filter_map(|(row, title)| (title == *text).then_some(row));
            match (found.next(), found.next()) {
                (None, _) => Resolution::Missing,
                (Some(row), None) => Resolution::Found(row),
                _ => Resolution::Ambiguous,
            }
        }
        Some(Anchor::Quote { text }) => {
            if text.is_empty() {
                return Resolution::Missing;
            }
            let Some(at) = body.find(text.as_str()) else {
                return Resolution::Missing;
            };
            // A second overlapping match is ambiguous too; str::match_indices
            // alone misses the second 'aa' in 'aaa'. Advance one Unicode char.
            let step = body[at..].chars().next().unwrap().len_utf8();
            if body[at + step..].contains(text.as_str()) {
                Resolution::Ambiguous
            } else {
                Resolution::Found(body[..at].bytes().filter(|&c| c == b'\n').count())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Excerpt {
    pub lines: Vec<String>,
    pub total_rows: usize,
    pub resolution: Resolution,
}
/// Streaming plain-Markdown preview. Retains only the visible rows, sanitizes
/// control characters, and never runs a Markdown renderer or opens a file.
pub fn excerpt(
    body: &str,
    width: usize,
    anchor: Option<&Anchor>,
    scroll: usize,
    height: usize,
) -> Excerpt {
    let resolution = resolve(body, anchor);
    if matches!(resolution, Resolution::Missing | Resolution::Ambiguous) {
        return Excerpt {
            lines: vec![],
            total_rows: 0,
            resolution,
        };
    }
    let start = if let Resolution::Found(line) = resolution {
        line
    } else {
        0
    };
    let mut out = Excerpt {
        lines: vec![],
        total_rows: 0,
        resolution,
    };
    let width = width.max(1);
    let mut emit = |row: String| {
        if out.total_rows >= scroll && out.lines.len() < height {
            out.lines.push(row);
        }
        out.total_rows = out.total_rows.saturating_add(1);
    };
    for line in body.lines().skip(start) {
        let mut row = String::new();
        let mut used = 0;
        for original in line.chars() {
            let ch = if original.is_control() {
                '�'
            } else {
                original
            };
            let w = ch.width().unwrap_or(0);
            if used + w > width && !row.is_empty() {
                emit(std::mem::take(&mut row));
                used = 0;
            }
            row.push(ch);
            used += w;
        }
        emit(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "wd-pins-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(p.join("notes")).unwrap();
            fs::write(p.join("notes/source.md"), "## Plan\nUnique source words.\n").unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn references_round_trip_without_copying_or_modifying_source() {
        let f = Fixture::new();
        let original = fs::read(f.0.join("notes/source.md")).unwrap();
        let mut s = Store::load(&f.0).unwrap();
        let id = s.update(|s| s.add("notes/source.md".into())).unwrap();
        s.update(|s| {
            let p = s.pin_mut(id)?;
            p.anchor = Some(Anchor::Heading {
                text: "Plan".into(),
            });
            p.mode = Mode::Anchored;
            p.scroll = 5;
            p.color = Color::Green;
            p.category = "Ideas".into();
            Ok(())
        })
        .unwrap();
        assert_eq!(Store::load(&f.0).unwrap().state, s.state);
        let json = fs::read_to_string(Store::path(&f.0)).unwrap();
        assert!(!json.contains("Unique source words"));
        assert_eq!(fs::read(f.0.join("notes/source.md")).unwrap(), original);
    }
    #[test]
    fn duplicate_cards_keep_independent_scroll_and_recoverable_life() {
        let f = Fixture::new();
        let mut s = Store::load(&f.0).unwrap();
        let a = s.update(|s| s.add("notes/source.md".into())).unwrap();
        let b = s.update(|s| s.add("notes/source.md".into())).unwrap();
        s.update(|s| {
            s.pin_mut(a)?.scroll = 7;
            s.set_life(a, Life::Trashed)?;
            s.set_life(b, Life::Archived)
        })
        .unwrap();
        s.update(|s| s.set_life(a, Life::Active)).unwrap();
        assert_eq!(s.state.pins[0].scroll, 7);
        assert_eq!(s.state.pins[1].scroll, 0);
        assert!(f.0.join("notes/source.md").exists());
    }
    #[test]
    fn scopes_are_explicit_and_dashboard_is_separate() {
        let mut s = State::default();
        let id = s.add("notes/source.md".into()).unwrap();
        let p = s.pin_mut(id).unwrap();
        assert!(p.visible(None, None, false));
        assert!(!p.visible(None, None, true));
        p.scope = Scope::Scene;
        p.context = Some("manuscript/scene.md".into());
        assert!(p.visible(Some(Path::new("manuscript/scene.md")), None, false));
        assert!(!p.visible(Some(Path::new("notes/source.md")), None, false));
        p.scope = Scope::Chapter;
        p.context = Some("manuscript/chapter".into());
        assert!(p.visible(None, Some(Path::new("manuscript/chapter")), false));
        p.scope = Scope::Always;
        assert!(p.visible(None, None, true));
        p.scope = Scope::Hidden;
        assert!(!p.visible(None, None, false));
    }
    #[test]
    fn external_conflicts_rollback_memory_and_preserve_other_file() {
        let f = Fixture::new();
        let mut s = Store::load(&f.0).unwrap();
        let id = s.update(|s| s.add("notes/source.md".into())).unwrap();
        let old = s.state.clone();
        fs::write(Store::path(&f.0), "external changed configuration").unwrap();
        assert!(
            s.update(|s| {
                s.pin_mut(id)?.scroll = 4;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(s.state, old);
        assert_eq!(
            fs::read_to_string(Store::path(&f.0)).unwrap(),
            "external changed configuration"
        );
    }
    #[test]
    fn bad_unknown_future_and_traversal_files_are_not_normalized() {
        let f = Fixture::new();
        let path = Store::path(&f.0);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in [
            "bad json".into(),
            serde_json::to_string(&serde_json::json!({"version":99})).unwrap(),
            {
                let mut v = serde_json::to_value(State::default()).unwrap();
                v["future"] = true.into();
                v.to_string()
            },
        ] {
            fs::write(&path, &text).unwrap();
            assert!(Store::load(&f.0).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        for path in [
            "../outside.md",
            "/absolute.md",
            ".grimoire/recovery/file.md",
            "notes/file.txt",
        ] {
            assert!(State::default().add(path.into()).is_err());
        }
    }
    #[test]
    fn failed_edit_or_bad_geometry_does_not_change_memory() {
        let f = Fixture::new();
        let mut s = Store::load(&f.0).unwrap();
        assert!(
            s.update(|s| {
                s.sidebar_width = 1;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(s.state, State::default());
        assert!(!Store::path(&f.0).exists());
    }
    #[test]
    fn heading_and_quote_anchors_move_and_fail_visibly() {
        let a = Anchor::Heading {
            text: "Plan".into(),
        };
        assert_eq!(
            resolve("intro\n## Plan\nwords", Some(&a)),
            Resolution::Found(1)
        );
        assert_eq!(
            resolve("new\nintro\n## Plan\nwords", Some(&a)),
            Resolution::Found(2)
        );
        assert_eq!(resolve("## Other", Some(&a)), Resolution::Missing);
        assert_eq!(resolve("## Plan\n## Plan", Some(&a)), Resolution::Ambiguous);
        let q = Anchor::Quote {
            text: "Café λ\nβeta".into(),
        };
        assert_eq!(
            resolve("preface\nCafé λ\nβeta remains", Some(&q)),
            Resolution::Found(1)
        );
        assert_eq!(
            resolve("Café λ\nβeta\nCafé λ\nβeta", Some(&q)),
            Resolution::Ambiguous
        );
    }
    #[test]
    fn previews_are_bounded_unicode_safe_and_sanitize_terminal_controls() {
        let body = "Café λ\n你好吗\n\u{1b}[2J\nlast";
        let e = excerpt(body, 4, None, 1, 3);
        assert_eq!(e.lines.len(), 3);
        assert!(e.lines.iter().all(|l| !l.contains('\u{1b}')));
        assert_eq!(
            excerpt(
                "## Plan\nword",
                30,
                Some(&Anchor::Heading {
                    text: "missing".into()
                }),
                0,
                2
            )
            .resolution,
            Resolution::Missing
        );
        let large = "words\n".repeat(10000);
        assert_eq!(excerpt(&large, 30, None, 5000, 4).lines.len(), 4);
    }
    #[cfg(unix)]
    #[test]
    fn aliases_have_same_identity_but_outside_symlinks_and_config_escape_are_refused() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let outside = Fixture::new();
        symlink(f.0.join("notes/source.md"), f.0.join("notes/alias.md")).unwrap();
        fs::hard_link(f.0.join("notes/source.md"), f.0.join("notes/hard.md")).unwrap();
        let a = identify(&f.0, Path::new("notes/source.md")).unwrap();
        for p in ["notes/alias.md", "notes/hard.md"] {
            assert!(a.same_file(&identify(&f.0, Path::new(p)).unwrap()));
        }
        symlink(
            outside.0.join("notes/source.md"),
            f.0.join("notes/outside.md"),
        )
        .unwrap();
        assert!(identify(&f.0, Path::new("notes/outside.md")).is_err());
        symlink(&outside.0, f.0.join(".grimoire")).unwrap();
        assert!(Store::load(&f.0).is_err());
        assert!(!outside.0.join("writingdeck").exists());
    }

    #[test]
    fn headings_ignore_fences_indented_code_and_normalize_optional_closing_hashes() {
        let body = "```md\n## Not a heading\n```\n~~~\n# Also code\n~~~\n    ## Indented code\n## Real heading ##\n";
        assert_eq!(headings(body), vec!["Real heading"]);
        assert_eq!(
            resolve(
                body,
                Some(&Anchor::Heading {
                    text: "Real heading".into()
                })
            ),
            Resolution::Found(7)
        );
        assert_eq!(
            resolve("aaa", Some(&Anchor::Quote { text: "aa".into() })),
            Resolution::Ambiguous
        );
        assert_eq!(
            resolve(
                "λλλ",
                Some(&Anchor::Quote {
                    text: "λλ".into()
                })
            ),
            Resolution::Ambiguous
        );
        let missing = excerpt(
            "Body must not impersonate an anchor",
            30,
            Some(&Anchor::Heading {
                text: "Absent".into(),
            }),
            0,
            8,
        );
        assert_eq!(missing.resolution, Resolution::Missing);
        assert!(missing.lines.is_empty());
    }
    #[test]
    fn unknown_fields_at_every_pin_level_and_invalid_contexts_are_preserved() {
        let f = Fixture::new();
        let mut s = Store::load(&f.0).unwrap();
        let id = s.update(|s| s.add("notes/source.md".into())).unwrap();
        s.update(|s| {
            s.pin_mut(id)?.anchor = Some(Anchor::Heading {
                text: "Plan".into(),
            });
            Ok(())
        })
        .unwrap();
        let original = serde_json::to_value(&s.state).unwrap();
        let path = Store::path(&f.0);
        for level in 0..3 {
            let mut value = original.clone();
            match level {
                0 => value["future"] = true.into(),
                1 => value["pins"][0]["future"] = true.into(),
                _ => value["pins"][0]["anchor"]["future"] = true.into(),
            };
            let text = value.to_string();
            fs::write(&path, &text).unwrap();
            assert!(Store::load(&f.0).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
        for context in [None, Some(PathBuf::from("../escape"))] {
            let mut state = s.state.clone();
            state.pins[0].scope = Scope::Scene;
            state.pins[0].context = context;
            assert!(state.validate().is_err());
        }
        let mut duplicate = s.state.clone();
        duplicate.pins.push(duplicate.pins[0].clone());
        assert!(duplicate.validate().is_err());
    }
    #[test]
    fn oversized_metadata_is_read_with_a_hard_bound_and_never_rewritten() {
        let f = Fixture::new();
        let path = Store::path(&f.0);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = vec![b'x'; 1_048_580];
        fs::write(&path, &bytes).unwrap();
        assert!(Store::load(&f.0).is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes.len() as u64);
    }
    #[test]
    fn io_failure_and_deleted_sources_do_not_destroy_existing_card_metadata() {
        let f = Fixture::new();
        let mut store = Store::load(&f.0).unwrap();
        let id = store.update(|s| s.add("notes/source.md".into())).unwrap();
        fs::remove_file(f.0.join("notes/source.md")).unwrap();
        assert!(identify(&f.0, Path::new("notes/source.md")).is_err());
        assert_eq!(Store::load(&f.0).unwrap().state, store.state);
        store.update(|s| s.set_life(id, Life::Archived)).unwrap();
        let old = store.state.clone();
        let dir = Store::path(&f.0).parent().unwrap().to_path_buf();
        let kept = dir.with_extension("kept");
        fs::rename(&dir, &kept).unwrap();
        fs::write(&dir, "not a directory").unwrap();
        assert!(
            store
                .update(|s| {
                    s.pin_mut(id)?.scroll = 9;
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(store.state, old);
        assert!(kept.join("pins.json").exists());
    }
    #[cfg(unix)]
    #[test]
    fn late_metadata_symlink_escape_and_source_alias_into_recovery_are_refused() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let outside = Fixture::new();
        let mut store = Store::load(&f.0).unwrap();
        symlink(&outside.0, f.0.join(".grimoire")).unwrap();
        assert!(store.update(|s| s.add("notes/source.md".into())).is_err());
        assert_eq!(store.state, State::default());
        assert!(!outside.0.join("writingdeck").exists());
        fs::remove_file(f.0.join(".grimoire")).unwrap();
        fs::create_dir_all(f.0.join(".grimoire/recovery")).unwrap();
        fs::write(f.0.join(".grimoire/recovery/source.md"), "kept words").unwrap();
        symlink(
            f.0.join(".grimoire/recovery/source.md"),
            f.0.join("notes/alias.md"),
        )
        .unwrap();
        assert!(identify(&f.0, Path::new("notes/alias.md")).is_err());
    }
}
