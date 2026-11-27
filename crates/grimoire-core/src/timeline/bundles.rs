//! Append-only bundle series and recovery of the local timeline.

use super::{Entry, configure_repository, init_repository, record, stage, timeline_head};
use crate::sessions::{self, Store, git};
use crate::{paths, resume};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use std::collections::HashSet;
use std::collections::hash_map::RandomState;
use std::ffi::OsStr;
use std::fs;
use std::hash::BuildHasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The name of this installation's bundle series.
pub fn machine_name() -> String {
    static LOCK: Mutex<()> = Mutex::new(());
    static CANDIDATE: OnceLock<String> = OnceLock::new();
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = paths::data_dir();
    let path = dir.join("machine-id");
    if let Some(name) = stored_machine_name(&path) {
        return name;
    }

    let name = CANDIDATE.get_or_init(|| {
        let host: String = resume::machine_name()
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let mut random = [0; 2];
        let hex = match fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut random)) {
            Ok(()) => u16::from_be_bytes(random),
            Err(_) => RandomState::new().hash_one(std::process::id()) as u16,
        };
        format!("{host}-{hex:04x}")
    });
    // Publish a complete file without replacing one made by another process.
    let scratch = dir.join(format!(".machine-id.{}.tmp", std::process::id()));
    if fs::create_dir_all(&dir).is_ok() && crate::atomic::write_text(&scratch, name).is_ok() {
        let published = fs::hard_link(&scratch, &path).is_ok();
        let _ = fs::remove_file(scratch);
        if published && cfg!(unix) {
            let _ = fs::File::open(&dir).and_then(|dir| dir.sync_all());
        }
    }
    stored_machine_name(&path).unwrap_or_else(|| name.clone())
}

fn stored_machine_name(path: &Path) -> Option<String> {
    let name = fs::read_to_string(path).ok()?;
    let name = name.trim();
    valid_machine(name).then(|| name.to_string())
}

fn valid_machine(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Debug)]
pub struct Bundle {
    pub number: u32,
    pub path: PathBuf,
    pub name: String,
    pub full: bool,
    pub tip: String,
}

const UPLOADED: &str = "refs/wros/uploaded/";
const RECEIPT: &str = "grimoire-uploaded-refs";

/// Create and verify the next bundle for this machine's series.
pub fn next_bundle(
    root: &Path,
    out_dir: &Path,
    on_drive: &[u32],
    now: DateTime<Local>,
) -> Result<Option<Bundle>> {
    let root = &std::path::absolute(root)?;
    let tip = timeline_head(root)?;
    let Store::Outside(repo) = sessions::store(root) else {
        bail!("the timeline isn't enabled for {}", root.display());
    };
    let uploaded = git(
        root,
        [
            "for-each-ref",
            "--format=%(refname:strip=3) %(objectname)",
            UPLOADED,
        ],
    )
    .text()?;
    let mut uploaded: Vec<_> = uploaded
        .lines()
        .filter_map(|line| {
            let (number, hash) = line.split_once(' ')?;
            Some((number.parse::<u32>().ok()?, hash))
        })
        .collect();
    uploaded.sort_unstable_by_key(|&(number, _)| number);

    let full = uploaded.is_empty() || uploaded.iter().any(|(n, _)| !on_drive.contains(n));
    let mut args: Vec<&OsStr> = vec![OsStr::new("bundle"), OsStr::new("create")];
    let mut exclude = None;
    if let Some(&(number, base)) = uploaded.last() {
        let receipt = fs::read_to_string(repo.join(RECEIPT)).unwrap_or_default();
        let (kind, refs) = receipt
            .strip_prefix(&format!("{number} {base} "))
            .and_then(|rest| rest.split_once('\n'))
            .unwrap_or_default();
        let covered = !full || (kind == "full" && on_drive.contains(&number));
        let range = format!("^{base}");
        if covered && !has_new_objects(root, &tip, &range, refs)? {
            return Ok(None);
        }
        if !full {
            exclude = Some(range);
        }
    }

    let highest = uploaded.last().map_or(0, |&(number, _)| number);
    let number = highest
        .max(on_drive.iter().copied().max().unwrap_or(0))
        .checked_add(1)
        .filter(|&n| n <= 999_999)
        .context("the timeline bundle series has reached its limit")?;
    let name = format!("{number:06} {}.bundle", now.format("%Y-%m-%d %H%M"));
    fs::create_dir_all(out_dir)?;
    let path = std::path::absolute(out_dir)?.join(&name);
    args.extend([path.as_os_str(), OsStr::new("--all")]);
    args.extend(exclude.as_deref().map(OsStr::new));
    git(root, args).stdout()?;
    git(
        root,
        [OsStr::new("bundle"), OsStr::new("verify"), path.as_os_str()],
    )
    .stdout()?;
    let heads = bundle_heads(root, &path)?;
    let tip = heads
        .lines()
        .find_map(|line| line.strip_suffix(" HEAD"))
        .map_or(tip, String::from);
    Ok(Some(Bundle {
        number,
        path,
        name,
        full,
        tip,
    }))
}

fn has_new_objects(root: &Path, tip: &str, range: &str, uploaded: &str) -> Result<bool> {
    let objects = git(
        root,
        ["rev-list", "--objects", "--no-object-names", "--all", range],
    )
    .text()?;
    if objects.is_empty() {
        return Ok(false);
    }
    // Annotated tags and other machines' lines can still appear in this range after
    // upload. Compare the refs Git actually advertised in the last successful bundle.
    let objects: HashSet<_> = objects.lines().collect();
    if objects.contains(tip) {
        return Ok(true);
    }
    let uploaded: HashSet<_> = uploaded.lines().collect();
    let refs = git(root, ["for-each-ref", "--format=%(objectname) %(refname)"]).text()?;
    Ok(refs.lines().any(|line| {
        line.split_once(' ').is_some_and(|(hash, name)| {
            !name.starts_with(UPLOADED) && objects.contains(hash) && !uploaded.contains(line)
        })
    }))
}

fn bundle_heads(root: &Path, path: &Path) -> Result<String> {
    let path = std::path::absolute(path)?;
    git(
        root,
        [
            OsStr::new("bundle"),
            OsStr::new("list-heads"),
            path.as_os_str(),
        ],
    )
    .text()
}

/// Remember a bundle only after its final name is on Drive.
pub fn mark_uploaded(root: &Path, bundle: &Bundle) -> Result<()> {
    let root = &std::path::absolute(root)?;
    let Store::Outside(repo) = sessions::store(root) else {
        bail!("the timeline isn't enabled for {}", root.display());
    };
    if bundle.path.is_file() {
        let heads = bundle_heads(root, &bundle.path)?;
        let kind = if bundle.full { "full" } else { "incremental" };
        crate::atomic::write_text(
            &repo.join(RECEIPT),
            &format!("{} {} {kind}\n{heads}\n", bundle.number, bundle.tip),
        )?;
    }
    let name = format!("{UPLOADED}{}", bundle.number);
    git(root, ["update-ref", &name, &bundle.tip]).stdout()?;
    Ok(())
}

/// Whether the local timeline's HEAD or the objects it references are unreadable.
pub fn is_damaged(root: &Path) -> bool {
    let Ok(root) = std::path::absolute(root) else {
        return true;
    };
    timeline_head(&root).is_err()
        || git(&root, ["fsck", "--connectivity-only", "--no-dangling"])
            .stdout()
            .is_err()
}

/// Rebuild the timeline from each machine's bundles, in number order within its series.
pub fn rebuild(root: &Path, bundles: &[(String, PathBuf)]) -> Result<Option<String>> {
    let root = &std::path::absolute(root)?;
    if !root.is_dir() || root.join(".git").exists() {
        bail!(
            "{} isn't a project with an outside timeline",
            root.display()
        );
    }
    if !sessions::git_available() {
        bail!("git isn't installed, so the project's timeline isn't available");
    }
    let bundles = bundles
        .iter()
        .map(|(machine, path)| {
            if !valid_machine(machine) {
                bail!("invalid timeline machine name {machine:?}");
            }
            let path = std::path::absolute(path)?;
            if !path.is_file() {
                bail!("{} isn't a bundle file", path.display());
            }
            Ok((machine.as_str(), path))
        })
        .collect::<Result<Vec<_>>>()?;

    let taken = sessions::outside_dir(root)
        .is_some_and(|repo| repo.exists() && !sessions::claims(&repo, root));
    sessions::assign_id(root, taken)?;
    let repo = sessions::outside_dir(root).context("giving the project an id")?;
    let old = if repo.try_exists()? {
        Some(move_aside(&repo)?)
    } else {
        None
    };

    let result = rebuild_repository(root, &repo, &bundles);
    if let Err(error) = &result {
        // A failed fetch must not leave a half-rebuilt repository at the active path.
        if repo.try_exists()? {
            move_aside(&repo).with_context(|| format!("after rebuild failed: {error:#}"))?;
        }
        if let Some(old) = old {
            fs::rename(&old, &repo).with_context(|| format!("after rebuild failed: {error:#}"))?;
        }
    }
    result
}

fn move_aside(repo: &Path) -> Result<PathBuf> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S-%f");
    let mut suffix = 0;
    loop {
        let mut path = repo.as_os_str().to_os_string();
        path.push(format!(".damaged-{stamp}-{suffix}"));
        let path = PathBuf::from(path);
        if !path.try_exists()? {
            fs::rename(repo, &path).with_context(|| format!("moving {} aside", repo.display()))?;
            return Ok(path);
        }
        suffix += 1;
    }
}

fn rebuild_repository(
    root: &Path,
    repo: &Path,
    bundles: &[(&str, PathBuf)],
) -> Result<Option<String>> {
    init_repository(root, repo)?;
    configure_repository(root, repo)?;
    git(root, ["symbolic-ref", "HEAD", "refs/heads/main"]).stdout()?;
    for (i, (machine, path)) in bundles.iter().enumerate() {
        if let Err(error) = fetch_bundle(root, machine, path) {
            if i + 1 == bundles.len() {
                return Err(error);
            }
            // A later full bundle can supply a lost incremental bundle's objects.
            // Load the objects first so the refs are still fetched in series order.
            for (_, later) in &bundles[i + 1..] {
                let _ = git(
                    root,
                    [
                        OsStr::new("bundle"),
                        OsStr::new("unbundle"),
                        later.as_os_str(),
                    ],
                )
                .stdout();
            }
            fetch_bundle(root, machine, path).with_context(|| format!("{error:#}"))?;
        }
    }

    stage(root, repo)?;
    let tree = git(root, ["write-tree"]).text()?;
    let history = git(
        root,
        [
            "log",
            "--all",
            "--topo-order",
            "--no-show-signature",
            "--no-notes",
            "--format=%H %T %ct",
        ],
    )
    .text()?;
    let mut matching = None;
    let mut newest = None;
    for line in history.lines() {
        let mut fields = line.split_whitespace();
        let hash = fields.next().context("missing entry hash")?;
        let entry_tree = fields.next().context("missing entry tree")?;
        let when: i64 = fields.next().context("missing entry date")?.parse()?;
        if newest.is_none_or(|(date, _)| when > date) {
            newest = Some((when, hash));
        }
        if entry_tree == tree && matching.is_none_or(|(date, _)| when > date) {
            matching = Some((when, hash));
        }
    }
    if let Some((_, base)) = matching.or(newest) {
        git(root, ["update-ref", "refs/heads/main", base]).stdout()?;
        if matching.is_none() {
            record(root, &Entry::BroughtIn)?;
        }
    } else {
        sessions::commit(root, "Timeline begins")?;
    }
    Ok(Some(timeline_head(root)?))
}

fn fetch_bundle(root: &Path, machine: &str, path: &Path) -> Result<()> {
    let refspec = format!("+refs/*:refs/machines/{machine}/*");
    git(
        root,
        [
            OsStr::new("fetch"),
            OsStr::new("--atomic"),
            OsStr::new("--no-tags"),
            OsStr::new("--no-write-fetch-head"),
            OsStr::new("--no-auto-maintenance"),
            OsStr::new("--no-recurse-submodules"),
            OsStr::new("--"),
            path.as_os_str(),
            OsStr::new(&refspec),
        ],
    )
    .stdout()
    .with_context(|| format!("fetching {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::{cleanup, temp_dir, write};
    use crate::timeline::{enable, mark_session};
    use chrono::TimeZone;
    use std::io::{BufRead, BufReader};

    const SCENE: &str = "chapters/01.md";

    fn date(day: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, day, 19, 15, 0).unwrap()
    }

    #[expect(
        clippy::print_stderr,
        reason = "this test fixture explains a skip when git is unavailable"
    )]
    fn book(tag: &str) -> Option<PathBuf> {
        if !sessions::git_available() {
            eprintln!("skipping {tag}: git isn't installed");
            return None;
        }
        let dir = temp_dir(tag);
        write(&dir, "book/chapters/01.md", "First draft.\n");
        enable(&dir.join("book")).unwrap();
        Some(dir)
    }

    fn clean_book(root: &Path, dir: &Path) {
        cleanup(&[&sessions::outside_dir(root).unwrap(), dir]);
    }

    fn save(root: &Path, text: &str) {
        write(root, SCENE, text);
        record(root, &Entry::Save).unwrap().unwrap();
    }

    fn upload(root: &Path, dir: &Path, on_drive: &[u32], day: u32) -> Bundle {
        let bundle = next_bundle(root, dir, on_drive, date(day))
            .unwrap()
            .unwrap();
        assert_eq!(bundle.path, dir.join(&bundle.name));
        assert_eq!(bundle.tip, timeline_head(root).unwrap());
        mark_uploaded(root, &bundle).unwrap();
        bundle
    }

    fn prerequisites(bundle: &Bundle) -> Vec<String> {
        BufReader::new(fs::File::open(&bundle.path).unwrap())
            .lines()
            .map(|line| line.unwrap())
            .take_while(|line| !line.is_empty())
            .filter_map(|line| Some(line.strip_prefix('-')?.split_whitespace().next()?.into()))
            .collect()
    }

    #[test]
    fn incremental_bundles_have_no_gap_or_overlap() {
        let Some(dir) = book("timeline-bundle-series") else {
            return;
        };
        let root = dir.join("book");
        let out = dir.join("bundles");
        let first = upload(&root, &out, &[], 16);
        save(&root, "Second draft.\n");
        let second = upload(&root, &out, &[1], 16);
        save(&root, "Third draft.\n");
        let third = upload(&root, &out, &[1, 2], 16);

        assert_eq!([first.number, second.number, third.number], [1, 2, 3]);
        assert!(first.full && !second.full && !third.full);
        assert!(prerequisites(&first).is_empty());
        assert_eq!(prerequisites(&second), [first.tip.as_str()]);
        assert_eq!(prerequisites(&third), [second.tip.as_str()]);

        let clone = dir.join("clone.git");
        git(
            &dir,
            [
                OsStr::new("clone"),
                OsStr::new("--bare"),
                OsStr::new("--"),
                first.path.as_os_str(),
                clone.as_os_str(),
            ],
        )
        .stdout()
        .unwrap();
        for bundle in [&second, &third] {
            git(
                &clone,
                [
                    OsStr::new("fetch"),
                    OsStr::new("--"),
                    bundle.path.as_os_str(),
                    OsStr::new("+refs/heads/main:refs/heads/main"),
                ],
            )
            .stdout()
            .unwrap();
        }
        assert_eq!(
            git(&clone, ["rev-list", "--reverse", "HEAD"])
                .text()
                .unwrap(),
            format!("{}\n{}\n{}", first.tip, second.tip, third.tip)
        );
        clean_book(&root, &dir);
    }

    #[test]
    fn full_bundle_when_drive_lost_one() {
        let Some(dir) = book("timeline-bundle-lost") else {
            return;
        };
        let root = dir.join("book");
        let out = dir.join("bundles");
        upload(&root, &out, &[], 16);
        save(&root, "Second draft.\n");
        upload(&root, &out, &[1], 16);
        let repaired = upload(&root, &out, &[2], 16);
        assert_eq!(repaired.number, 3);
        assert!(repaired.full);
        assert!(prerequisites(&repaired).is_empty());
        assert!(
            next_bundle(&root, &out, &[2, 3], date(16))
                .unwrap()
                .is_none()
        );
        clean_book(&root, &dir);
    }

    #[test]
    fn numbers_follow_the_series_not_the_clock() {
        let Some(dir) = book("timeline-bundle-clock") else {
            return;
        };
        let root = dir.join("book");
        let first = upload(&root, &dir, &[6], 17);
        save(&root, "Second draft.\n");
        let second = upload(&root, &dir, &[6, 7], 16);
        assert_eq!(first.name, "000007 2026-11-17 1915.bundle");
        assert_eq!(second.name, "000008 2026-11-16 1915.bundle");
        clean_book(&root, &dir);
    }

    #[test]
    fn nothing_new_no_bundle() {
        let Some(dir) = book("timeline-bundle-unchanged") else {
            return;
        };
        let root = dir.join("book");
        upload(&root, &dir, &[], 16);
        assert!(next_bundle(&root, &dir, &[1], date(17)).unwrap().is_none());
        clean_book(&root, &dir);
    }

    #[test]
    fn a_session_tag_without_a_new_commit_is_bundled_once() {
        let Some(dir) = book("timeline-bundle-tag") else {
            return;
        };
        let root = dir.join("book");
        let first = upload(&root, &dir, &[], 16);
        let tag = mark_session(&root, date(16)).unwrap().unwrap();
        let second = upload(&root, &dir, &[1], 17);
        assert!(!second.full);
        assert_eq!(second.tip, first.tip);
        assert!(
            bundle_heads(&root, &second.path)
                .unwrap()
                .ends_with(&format!("refs/tags/{tag}"))
        );
        assert!(
            next_bundle(&root, &dir, &[1, 2], date(17))
                .unwrap()
                .is_none()
        );
        clean_book(&root, &dir);
    }

    #[test]
    fn rebuild_from_two_machines_keeps_both_lines_and_matches_current_files() {
        let Some(left) = book("timeline-rebuild-left") else {
            return;
        };
        let right = book("timeline-rebuild-right").unwrap();
        let root = left.join("book");
        let other = right.join("book");
        let matching = timeline_head(&root).unwrap();
        save(&root, "Newer files from the first laptop.\n");
        let first = upload(&root, &left, &[], 16);
        save(&other, "Files from the second laptop.\n");
        let tag = mark_session(&other, date(16)).unwrap().unwrap();
        let second = upload(&other, &right, &[], 16);
        write(&root, SCENE, "First draft.\n");

        let head = rebuild(
            &root,
            &[("left".into(), first.path), ("right".into(), second.path)],
        )
        .unwrap();
        assert_eq!(head, Some(matching));
        assert_eq!(
            git(&root, ["symbolic-ref", "HEAD"]).text().unwrap(),
            "refs/heads/main"
        );
        for (machine, tip) in [("left", first.tip), ("right", second.tip)] {
            let branch = format!("refs/machines/{machine}/heads/main");
            assert_eq!(git(&root, ["rev-parse", &branch]).text().unwrap(), tip);
        }
        let tag_ref = format!("refs/machines/right/tags/{tag}");
        let refs = git(&root, ["for-each-ref", "--format=%(refname)"])
            .text()
            .unwrap();
        assert!(refs.lines().any(|line| line == tag_ref));
        assert!(!refs.lines().any(|line| line.starts_with("refs/tags/")));
        assert_eq!(
            fs::read_to_string(root.join(SCENE)).unwrap(),
            "First draft.\n"
        );
        cleanup(&[
            &sessions::outside_dir(&root).unwrap(),
            &damaged_copy(&root),
            &left,
        ]);
        clean_book(&other, &right);
    }

    #[test]
    fn rebuild_records_brought_in_when_files_differ() {
        let Some(dir) = book("timeline-rebuild-brought-in") else {
            return;
        };
        let root = dir.join("book");
        let older = book("timeline-rebuild-older").unwrap();
        let other = older.join("book");
        for (book, when) in [(&other, "1700000000 +0000"), (&root, "1800000000 +0000")] {
            sessions::git_with_identity(book, &["commit", "--amend", "--no-edit"])
                .unwrap()
                .env("GIT_COMMITTER_DATE", when)
                .stdout()
                .unwrap();
        }
        let old_bundle = upload(&other, &older, &[], 16);
        let new_bundle = upload(&root, &dir, &[], 16);
        write(&root, SCENE, "Changed on the phone.\n");
        let head = rebuild(
            &root,
            &[
                ("newer".into(), new_bundle.path),
                ("older".into(), old_bundle.path),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(head, timeline_head(&root).unwrap());
        assert_eq!(
            git(&root, ["rev-parse", "HEAD^"]).text().unwrap(),
            new_bundle.tip
        );
        assert_eq!(
            git(&root, ["log", "-1", "--format=%s"]).text().unwrap(),
            "Brought in from Drive"
        );
        assert_eq!(
            git(&root, ["show", &format!("HEAD:{SCENE}")])
                .text()
                .unwrap(),
            "Changed on the phone."
        );
        cleanup(&[&damaged_copy(&root)]);
        clean_book(&root, &dir);
        clean_book(&other, &older);
    }

    fn damaged_copy(root: &Path) -> PathBuf {
        let repo = sessions::outside_dir(root).unwrap();
        let prefix = format!("{}.damaged-", repo.file_name().unwrap().to_string_lossy());
        fs::read_dir(repo.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap())
            .find(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            .unwrap()
            .path()
    }

    #[test]
    fn damaged_repository_is_moved_aside_not_deleted() {
        let Some(dir) = book("timeline-rebuild-damaged") else {
            return;
        };
        let root = dir.join("book");
        let bundle = upload(&root, &dir, &[], 16);
        let repo = sessions::outside_dir(&root).unwrap();
        fs::write(repo.join("keep-me"), "old repository\n").unwrap();
        assert!(!is_damaged(&root));

        let tree = git(&root, ["rev-parse", "HEAD^{tree}"]).text().unwrap();
        let object = repo.join("objects").join(&tree[..2]).join(&tree[2..]);
        let bytes = fs::read(&object).unwrap();
        fs::remove_file(&object).unwrap();
        assert_eq!(timeline_head(&root).unwrap(), bundle.tip);
        assert!(is_damaged(&root));
        fs::write(object, bytes).unwrap();

        fs::remove_file(repo.join("HEAD")).unwrap();
        assert!(is_damaged(&root));

        assert_eq!(
            rebuild(&root, &[("laptop".into(), bundle.path)]).unwrap(),
            Some(bundle.tip)
        );
        assert_eq!(sessions::outside_dir(&root).unwrap(), repo);
        assert!(!is_damaged(&root));
        let damaged = damaged_copy(&root);
        assert_eq!(
            fs::read_to_string(damaged.join("keep-me")).unwrap(),
            "old repository\n"
        );
        cleanup(&[&damaged]);
        clean_book(&root, &dir);
    }

    #[test]
    fn rebuild_uses_a_later_full_bundle_when_a_prerequisite_was_lost() {
        let Some(dir) = book("timeline-rebuild-gap") else {
            return;
        };
        let root = dir.join("book");
        let first = upload(&root, &dir, &[], 16);
        save(&root, "Second draft.\n");
        upload(&root, &dir, &[1], 16);
        save(&root, "Third draft.\n");
        let third = upload(&root, &dir, &[1, 2], 16);
        let fourth = upload(&root, &dir, &[1, 3], 16);
        assert!(fourth.full);
        assert_eq!(
            rebuild(
                &root,
                &[
                    ("laptop".into(), first.path),
                    ("laptop".into(), third.path),
                    ("laptop".into(), fourth.path),
                ],
            )
            .unwrap(),
            Some(fourth.tip)
        );
        assert_eq!(
            git(&root, ["rev-list", "--count", "HEAD"]).text().unwrap(),
            "3"
        );
        cleanup(&[&damaged_copy(&root)]);
        clean_book(&root, &dir);
    }

    #[test]
    fn machine_name_is_made_once() {
        let dir = temp_dir("timeline-machine-name");
        let path = paths::data_dir().join("machine-id");
        let _ = fs::remove_file(&path);
        let name = machine_name();
        let (host, suffix) = name.rsplit_once('-').unwrap();
        assert!(!host.is_empty());
        assert!(
            host.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        );
        assert_eq!(suffix.len(), 4);
        assert!(
            suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(fs::read_to_string(&path).unwrap().trim(), name);
        assert_eq!(machine_name(), name);
        fs::remove_file(&path).unwrap();
        cleanup(&[&dir]);
    }
}
