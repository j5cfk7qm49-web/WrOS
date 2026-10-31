//! Scene history: dated copies of a scene, kept as you work.
//!
//! Autosave writes every couple of seconds, which is far too often to keep
//! every version, so a new copy is kept at most every few minutes — plus the
//! scene as it was on disk before this session first changed it, and a copy
//! before anything drastic (restoring an old version, replacing across the
//! book). Copies live at `.grimoire/history/<scene path>/<date>_<time>.md`:
//! ordinary Markdown files, readable without Grimoire.

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::project;

/// How far apart ordinary snapshots are kept.
pub const GAP: Duration = Duration::from_secs(5 * 60);

const STAMP: &str = "%Y-%m-%d_%H%M%S";

pub fn dir_for(root: &Path, scene: &Path) -> PathBuf {
    root.join(".grimoire")
        .join("history")
        .join(crate::names::nfc(&raw_rel(root, scene)))
}

/// The scene's place in the book as spelled on disk. History is kept under
/// the composed (NFC) spelling so a Mac that hands over `e` + accent and a
/// Linux machine that wrote `é` share one scene's versions; this is the
/// spelling older history may have been kept under.
fn raw_rel(root: &Path, scene: &Path) -> PathBuf {
    scene.strip_prefix(root).unwrap_or(scene).with_extension("")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub file: PathBuf,
    pub when: DateTime<Local>,
    pub text: String,
}

impl Version {
    pub fn body(&self) -> String {
        project::split_frontmatter(&self.text).1
    }

    pub fn words(&self) -> usize {
        crate::notes::count_words(&self.body())
    }
}

/// Every kept version of a scene, newest first.
pub fn versions(root: &Path, scene: &Path) -> Vec<Version> {
    let composed = dir_for(root, scene);
    let spelled = root
        .join(".grimoire")
        .join("history")
        .join(raw_rel(root, scene));
    let mut dirs = vec![composed.clone()];
    // The other spelling only counts when it's a different folder: on a Mac
    // (and in Box) both spellings open the same one, which would list every
    // version twice.
    if spelled != composed && !crate::names::same_file(&spelled, &composed) {
        dirs.push(spelled);
    }
    let mut out: Vec<Version> = dirs
        .iter()
        .filter_map(|d| fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten())
        .filter_map(|e| {
            let file = e.path();
            let stem = file.file_stem()?.to_string_lossy().to_string();
            // `2026-09-16_214107` or, for two in one second, `…_214107-2`.
            let stamp = stem.split('-').take(3).collect::<Vec<_>>().join("-");
            let naive = NaiveDateTime::parse_from_str(&stamp, STAMP).ok()?;
            let when = Local.from_local_datetime(&naive).earliest()?;
            let text = fs::read_to_string(&file).ok()?;
            Some(Version { file, when, text })
        })
        .collect();
    out.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| b.file.cmp(&a.file)));
    out
}

/// Keep a copy of `text` unless the newest copy already says exactly that, or
/// (when `gap` is given) the newest copy is more recent than the gap.
/// Returns whether a copy was written.
pub fn snapshot(root: &Path, scene: &Path, text: &str, gap: Option<Duration>) -> Result<bool> {
    snapshot_at(root, scene, text, gap, Local::now())
}

fn snapshot_at(
    root: &Path,
    scene: &Path,
    text: &str,
    gap: Option<Duration>,
    now: DateTime<Local>,
) -> Result<bool> {
    let newest = versions(root, scene).into_iter().next();
    if let Some(v) = &newest {
        if v.text == text {
            return Ok(false);
        }
        if let Some(gap) = gap {
            let age = now
                .signed_duration_since(v.when)
                .to_std()
                .unwrap_or_default();
            if age < gap {
                return Ok(false);
            }
        }
    }
    let dir = dir_for(root, scene);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let stamp = now.format(STAMP).to_string();
    let mut file = dir.join(format!("{stamp}.md"));
    let mut n = 2;
    while file.exists() {
        file = dir.join(format!("{stamp}-{n}.md"));
        n += 1;
    }
    project::write_atomic(&file, text)?;
    Ok(true)
}

/// Scenes or folders moved or were renamed: their history goes with them.
/// Several at once (a swap) go through temporary names, so two histories
/// trading places can't land on each other.
pub fn follow_all(root: &Path, renames: &[(PathBuf, PathBuf)]) {
    let staged: Vec<(PathBuf, PathBuf)> = renames
        .iter()
        .enumerate()
        .filter_map(|(i, (from, to))| {
            // Kept under the composed spelling, or under the one on disk if an
            // older Grimoire wrote it before names were normalised.
            let spelled = root
                .join(".grimoire")
                .join("history")
                .join(raw_rel(root, from));
            let old = [dir_for(root, from), spelled]
                .into_iter()
                .find(|d| d.exists())?;
            let tmp = old.with_file_name(format!(".moving-{i}"));
            fs::rename(&old, &tmp).ok()?;
            Some((tmp, dir_for(root, to)))
        })
        .collect();
    for (tmp, new) in staged {
        if let Some(parent) = new.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // History already waiting at the new path belongs to something that
        // was there once and isn't now: set it aside, so this scene's own
        // history isn't stranded under a temporary name or mixed with it.
        set_aside_dir(&new);
        let _ = fs::rename(&tmp, &new);
    }
}

/// A new scene is starting at `scene`: any history left there by one that
/// used the same path before is set aside (kept, never deleted), so the new
/// scene's list starts empty.
pub fn set_aside(root: &Path, scene: &Path) {
    set_aside_dir(&dir_for(root, scene));
}

fn set_aside_dir(dir: &Path) {
    if !dir.exists() {
        return;
    }
    let name = dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut n = 1;
    let mut to = dir.with_file_name(format!("{name}.orphaned-{n}"));
    while to.exists() {
        n += 1;
        to = dir.with_file_name(format!("{name}.orphaned-{n}"));
    }
    let _ = fs::rename(dir, &to);
}

/// One run of a word diff, for drawing.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Same(String),
    /// In the old version, gone now.
    Removed(String),
    /// New since the old version.
    Added(String),
}

/// What changed between an old version and the current text, word by word.
pub fn diff(old: &str, new: &str) -> Vec<Piece> {
    use similar::{Algorithm, ChangeTag, TextDiff};
    let d = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(Duration::from_millis(250))
        .diff_words(old, new);
    let mut out: Vec<Piece> = Vec::new();
    for c in d.iter_all_changes() {
        let v = c.value().to_string();
        let piece = match c.tag() {
            ChangeTag::Equal => Piece::Same(v),
            ChangeTag::Delete => Piece::Removed(v),
            ChangeTag::Insert => Piece::Added(v),
        };
        // Merge neighbours of the same kind so runs draw as runs.
        match (out.last_mut(), &piece) {
            (Some(Piece::Same(a)), Piece::Same(b))
            | (Some(Piece::Removed(a)), Piece::Removed(b))
            | (Some(Piece::Added(a)), Piece::Added(b)) => a.push_str(b),
            _ => out.push(piece),
        }
    }
    out
}

/// A transient change between one reviewed saved draft and the current body.
/// Byte positions are never persisted or used against an unverified revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub current_range: std::ops::Range<usize>,
    pub saved: String,
    pub current: String,
}

/// Group neighbouring word-diff changes, retaining unchanged writing between
/// groups. Uses the existing history diff; this is not a new comparison engine.
pub fn passages(pieces: &[Piece], current: &str) -> Result<Vec<Passage>> {
    use anyhow::ensure;
    ensure!(
        current.len() <= 2 * 1024 * 1024 && pieces.len() <= 16384,
        "passage review exceeds 2 MiB or 16384 diff runs; no partial review"
    );
    let mut passages = Vec::new();
    let mut pending: Option<Passage> = None;
    let mut cursor = 0usize;
    let mut saved_bytes = 0usize;
    let finish = |pending: &mut Option<Passage>, passages: &mut Vec<Passage>| -> Result<()> {
        if let Some(passage) = pending.take() {
            ensure!(
                passages.len() < 4096,
                "passage review exceeds 4096 changes; no partial review"
            );
            passages.push(passage);
        }
        Ok(())
    };
    for piece in pieces {
        let (text, consumes_current, consumes_saved) = match piece {
            Piece::Same(s) => (s, true, true),
            Piece::Added(s) => (s, true, false),
            Piece::Removed(s) => (s, false, true),
        };
        if consumes_saved {
            saved_bytes = saved_bytes.saturating_add(text.len());
            ensure!(
                saved_bytes <= 2 * 1024 * 1024,
                "saved draft exceeds 2 MiB passage review bound"
            );
        }
        let end = if consumes_current {
            cursor.saturating_add(text.len())
        } else {
            cursor
        };
        if consumes_current {
            ensure!(
                current.get(cursor..end) == Some(text.as_str()),
                "diff no longer matches current source; reopen the comparison"
            );
        }
        match piece {
            Piece::Same(_) => finish(&mut pending, &mut passages)?,
            Piece::Added(_) | Piece::Removed(_) => {
                let passage = pending.get_or_insert_with(|| Passage {
                    current_range: cursor..cursor,
                    saved: String::new(),
                    current: String::new(),
                });
                if consumes_current {
                    passage.current.push_str(text);
                }
                if consumes_saved {
                    passage.saved.push_str(text);
                }
                passage.current_range.end = end;
            }
        }
        cursor = end;
    }
    ensure!(
        cursor == current.len(),
        "diff no longer covers current source; reopen the comparison"
    );
    finish(&mut pending, &mut passages)?;
    Ok(passages)
}

/// Restore exactly one reviewed change after validating the whole current body.
pub fn restore_passage(pieces: &[Piece], current: &str, selected: usize) -> Result<String> {
    let passages = passages(pieces, current)?;
    let passage = passages
        .get(selected)
        .ok_or_else(|| anyhow::anyhow!("select a changed passage first"))?;
    let mut restored = current.to_owned();
    restored.replace_range(passage.current_range.clone(), &passage.saved);
    Ok(restored)
}

#[cfg(test)]
mod passage_tests {
    use super::*;
    #[test]
    fn selective_restore_changes_only_one_group_and_preserves_unicode_and_private_text() {
        let current = "λ green retained middle words gold end. %% private %%\n";
        let saved = "λ red retained middle words blue end. %% private %%\n";
        let pieces = diff(saved, current);
        let changes = passages(&pieces, current).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].current, "green");
        assert_eq!(changes[1].saved, "blue");
        assert_eq!(
            restore_passage(&pieces, current, 0).unwrap(),
            "λ red retained middle words gold end. %% private %%\n"
        );
        assert_eq!(
            restore_passage(&pieces, current, 1).unwrap(),
            "λ green retained middle words blue end. %% private %%\n"
        );
        assert!(restore_passage(&pieces, current, 2).is_err());
        assert!(restore_passage(&pieces, &current.replace("private", "changed"), 0).is_err());
        assert!(restore_passage(&pieces, &(current.to_owned() + "extra"), 0).is_err());
    }
    #[test]
    fn selective_restore_handles_empty_inserted_deleted_and_eof_changes() {
        for (saved, current) in [
            ("Saved λ", ""),
            ("", "Added λ"),
            ("λ end", "λ"),
            ("λ", "λ end"),
            ("α old ω", "α new ω"),
        ] {
            let pieces = diff(saved, current);
            assert_eq!(passages(&pieces, current).unwrap().len(), 1);
            assert_eq!(restore_passage(&pieces, current, 0).unwrap(), saved);
        }
        assert!(
            passages(&diff("same λ", "same λ"), "same λ")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn selective_restore_refuses_oversized_partial_or_malformed_reviews() {
        assert!(
            passages(
                &[Piece::Added("x".repeat(2 * 1024 * 1024 + 1))],
                &"x".repeat(2 * 1024 * 1024 + 1)
            )
            .is_err()
        );
        assert!(passages(&[Piece::Removed("x".repeat(2 * 1024 * 1024 + 1))], "").is_err());
        assert!(passages(&vec![Piece::Same("".into()); 16385], "").is_err());
        assert!(passages(&[Piece::Same("a".into())], "λ").is_err());
        let mut pieces = Vec::new();
        for _ in 0..4097 {
            pieces.extend([
                Piece::Same("x".into()),
                Piece::Removed("a".into()),
                Piece::Added("b".into()),
            ]);
        }
        assert!(
            passages(&pieces, &"xb".repeat(4097))
                .unwrap_err()
                .to_string()
                .contains("4096")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as Span;

    fn root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("grimoire-history-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copies_are_spaced_out_and_never_repeat_themselves() {
        let d = root("gap");
        let scene = d.join("manuscript/01-Act-One/01-Gravel.md");
        let t0 = Local.with_ymd_and_hms(2026, 9, 16, 21, 0, 0).unwrap();
        assert!(
            snapshot_at(&d, &scene, "one", Some(GAP), t0).unwrap(),
            "the first copy is always kept"
        );
        assert!(
            !snapshot_at(&d, &scene, "one two", Some(GAP), t0 + Span::seconds(30)).unwrap(),
            "too soon"
        );
        assert!(
            snapshot_at(&d, &scene, "one two", Some(GAP), t0 + Span::minutes(6)).unwrap(),
            "after the gap"
        );
        assert!(
            !snapshot_at(&d, &scene, "one two", None, t0 + Span::minutes(7)).unwrap(),
            "identical text is never kept twice"
        );
        assert!(
            snapshot_at(&d, &scene, "one two three", None, t0 + Span::minutes(7)).unwrap(),
            "no gap: kept straight away"
        );

        let v = versions(&d, &scene);
        assert_eq!(
            v.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(),
            ["one two three", "one two", "one"],
            "newest first"
        );
        assert!(
            v[0].file
                .starts_with(d.join(".grimoire/history/manuscript/01-Act-One/01-Gravel"))
        );
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn two_copies_in_the_same_second_both_survive() {
        let d = root("same-second");
        let scene = d.join("manuscript/01-Gravel.md");
        let t = Local.with_ymd_and_hms(2026, 9, 16, 21, 0, 0).unwrap();
        snapshot_at(&d, &scene, "a", None, t).unwrap();
        snapshot_at(&d, &scene, "b", None, t).unwrap();
        assert_eq!(versions(&d, &scene).len(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn history_follows_a_renamed_scene() {
        let d = root("follow");
        let from = d.join("manuscript/01-Gravel.md");
        let to = d.join("manuscript/02-Gravel.md");
        snapshot(&d, &from, "kept", None).unwrap();
        snapshot(&d, &to, "other", None).unwrap();
        // A swap: each takes the other's name, and neither history is lost.
        follow_all(
            &d,
            &[(from.clone(), to.clone()), (to.clone(), from.clone())],
        );
        assert_eq!(versions(&d, &to)[0].text, "kept");
        assert_eq!(versions(&d, &from)[0].text, "other");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn history_follows_onto_a_path_that_had_history_of_its_own() {
        let d = root("follow-onto");
        let from = d.join("manuscript/01-Gravel.md");
        let to = d.join("manuscript/03-Gravel.md");
        snapshot(&d, &from, "this scene's past", None).unwrap();
        // Left behind by a scene that used to be called 03-Gravel.
        snapshot(&d, &to, "someone else's past", None).unwrap();
        follow_all(&d, &[(from.clone(), to.clone())]);
        let v = versions(&d, &to);
        assert_eq!(v.len(), 1, "only its own history");
        assert_eq!(v[0].text, "this scene's past");
        let hidden: Vec<_> = fs::read_dir(d.join(".grimoire/history/manuscript"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with(".moving-"))
            .collect();
        assert!(hidden.is_empty(), "nothing stranded: {hidden:?}");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_new_scene_at_an_old_path_starts_with_no_history() {
        let d = root("set-aside");
        let scene = d.join("manuscript/03-Untitled.md");
        snapshot(&d, &scene, "the deleted one's words", None).unwrap();
        set_aside(&d, &scene);
        assert!(versions(&d, &scene).is_empty());
        let kept = d.join(".grimoire/history/manuscript/03-Untitled.orphaned-1");
        assert!(kept.is_dir(), "set aside, not deleted");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_diff_marks_what_went_and_what_came() {
        let pieces = diff("Wren ran across the lot.", "Wren crossed the lot slowly.");
        assert!(
            pieces.contains(&Piece::Removed("ran across".into()))
                || pieces
                    .iter()
                    .any(|p| matches!(p, Piece::Removed(s) if s.contains("ran")))
        );
        assert!(
            pieces
                .iter()
                .any(|p| matches!(p, Piece::Added(s) if s.contains("crossed")))
        );
        let rebuilt_new: String = pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Same(s) | Piece::Added(s) => Some(s.as_str()),
                Piece::Removed(_) => None,
            })
            .collect();
        assert_eq!(rebuilt_new, "Wren crossed the lot slowly.");
    }
}
