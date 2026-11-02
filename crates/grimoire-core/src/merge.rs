//! Three-way merge of Markdown by paragraph, for a file changed both in WrOS
//! and in Google Drive since they last agreed. A paragraph is a block between
//! blank lines; YAML frontmatter is one block. Changes on one side are taken;
//! the same paragraph changed differently on both sides is a clash for the
//! writer to settle, never a guess.

/// A paragraph changed differently on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub mine: String,
    pub theirs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Same(String),
    Clash(Clash),
}

/// How the writer settles one clash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Mine,
    Theirs,
    Both,
}

/// Above this many paragraph comparisons the whole file is one clash rather
/// than a slow alignment.
const ALIGN_LIMIT: usize = 4_000_000;

/// The blocks of `text`, without the blank lines between them.
pub fn paragraphs(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let mut lines = text.lines().peekable();
    let mut blocks = Vec::new();
    // Frontmatter stays one block even when it holds blank lines.
    if lines.peek() == Some(&"---") {
        let mut front = vec![lines.next().unwrap_or_default()];
        for line in lines.by_ref() {
            front.push(line);
            if line == "---" {
                break;
            }
        }
        blocks.push(front.join("\n"));
    }
    let mut current: Vec<&str> = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }
    blocks
}

/// For each paragraph of `a`, the index it matches in `b` on a longest common subsequence.
fn align(a: &[String], b: &[String]) -> Vec<Option<usize>> {
    let (n, m) = (a.len(), b.len());
    let mut lengths = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i][j] = if a[i] == b[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let mut matched = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            matched[i] = Some(j);
            i += 1;
            j += 1;
        } else if lengths[i + 1][j] >= lengths[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matched
}

/// Settle one stretch between paragraphs all three versions share.
fn resolve(base: &[String], mine: &[String], theirs: &[String], pieces: &mut Vec<Piece>) {
    let take = |side: &[String], pieces: &mut Vec<Piece>| {
        pieces.extend(side.iter().cloned().map(Piece::Same));
    };
    if mine == base {
        take(theirs, pieces);
    } else if theirs == base || mine == theirs {
        take(mine, pieces);
    } else if base.len() > 1 && mine.len() == base.len() && theirs.len() == base.len() {
        // Neighbouring edits, paragraph for paragraph: ask only where both changed.
        for i in 0..base.len() {
            resolve(&base[i..=i], &mine[i..=i], &theirs[i..=i], pieces);
        }
    } else {
        pieces.push(Piece::Clash(Clash {
            mine: mine.join("\n\n"),
            theirs: theirs.join("\n\n"),
        }));
    }
}

/// Paragraph pieces of the merge of `mine` and `theirs` against their common `base`.
pub fn three_way(base: &str, mine: &str, theirs: &str) -> Vec<Piece> {
    let (base, mine, theirs) = (paragraphs(base), paragraphs(mine), paragraphs(theirs));
    let mut pieces = Vec::new();
    let too_big = base.len().saturating_mul(mine.len().max(theirs.len())) > ALIGN_LIMIT;
    if too_big {
        resolve(&base, &mine, &theirs, &mut pieces);
        return pieces;
    }
    let (in_mine, in_theirs) = (align(&base, &mine), align(&base, &theirs));
    let (mut b, mut m, mut t) = (0, 0, 0);
    for anchor in 0..=base.len() {
        // An anchor is a base paragraph both sides kept, in order; the end is one too.
        let at = if anchor == base.len() {
            Some((mine.len(), theirs.len()))
        } else {
            match (in_mine[anchor], in_theirs[anchor]) {
                (Some(x), Some(y)) if x >= m && y >= t => Some((x, y)),
                _ => None,
            }
        };
        let Some((x, y)) = at else { continue };
        resolve(&base[b..anchor], &mine[m..x], &theirs[t..y], &mut pieces);
        if anchor < base.len() {
            pieces.push(Piece::Same(base[anchor].clone()));
        }
        (b, m, t) = (anchor + 1, x + 1, y + 1);
    }
    pieces
}

/// The merged text, one choice per clash in order; paragraphs joined by a blank line.
pub fn assemble(pieces: &[Piece], choices: &[Choice]) -> String {
    let mut choices = choices.iter();
    let mut blocks: Vec<&str> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Same(text) => blocks.push(text),
            Piece::Clash(clash) => match choices.next().copied().unwrap_or(Choice::Both) {
                Choice::Mine => blocks.push(&clash.mine),
                Choice::Theirs => blocks.push(&clash.theirs),
                Choice::Both => blocks.extend([clash.mine.as_str(), clash.theirs.as_str()]),
            },
        }
    }
    let text = blocks
        .into_iter()
        .filter(|b| !b.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() { text } else { text + "\n" }
}

/// How many clashes `pieces` holds.
pub fn clashes(pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .filter(|p| matches!(p, Piece::Clash(_)))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(base: &str, mine: &str, theirs: &str) -> (String, usize) {
        let pieces = three_way(base, mine, theirs);
        (
            assemble(&pieces, &vec![Choice::Mine; clashes(&pieces)]),
            clashes(&pieces),
        )
    }

    #[test]
    fn edits_to_different_paragraphs_combine_without_asking() {
        let base = "A\n\nB\n\nC\n";
        assert_eq!(
            merged(base, "A1\n\nB\n\nC\n", "A\n\nB\n\nC2\n"),
            ("A1\n\nB\n\nC2\n".into(), 0)
        );
        // Insertions in different places, one on each side.
        assert_eq!(
            merged(
                base,
                "A\n\nNew mine\n\nB\n\nC\n",
                "A\n\nB\n\nC\n\nNew theirs\n"
            ),
            ("A\n\nNew mine\n\nB\n\nC\n\nNew theirs\n".into(), 0)
        );
    }

    #[test]
    fn the_same_paragraph_changed_differently_is_a_clash_with_both_texts() {
        let pieces = three_way(
            "A\n\nB\n\nC\n",
            "A\n\nB mine\n\nC\n",
            "A\n\nB theirs\n\nC\n",
        );
        assert_eq!(clashes(&pieces), 1);
        assert!(pieces.contains(&Piece::Clash(Clash {
            mine: "B mine".into(),
            theirs: "B theirs".into()
        })));
        assert_eq!(assemble(&pieces, &[Choice::Theirs]), "A\n\nB theirs\n\nC\n");
        assert_eq!(
            assemble(&pieces, &[Choice::Both]),
            "A\n\nB mine\n\nB theirs\n\nC\n"
        );
        assert_eq!(assemble(&pieces, &[Choice::Mine]), "A\n\nB mine\n\nC\n");
    }

    #[test]
    fn neighbouring_edits_ask_only_about_the_paragraph_both_changed() {
        let pieces = three_way(
            "A\n\nB\n\nC\n",
            "A mine\n\nB\n\nC\n",
            "A theirs\n\nB theirs\n\nC\n",
        );
        assert_eq!(clashes(&pieces), 1);
        assert_eq!(
            assemble(&pieces, &[Choice::Mine]),
            "A mine\n\nB theirs\n\nC\n"
        );
    }

    #[test]
    fn identical_edits_and_one_sided_deletions_need_no_choice() {
        assert_eq!(
            merged("A\n\nB\n", "A\n\nB2\n", "A\n\nB2\n"),
            ("A\n\nB2\n".into(), 0)
        );
        assert_eq!(
            merged("A\n\nB\n\nC\n", "A\n\nC\n", "A\n\nB\n\nC\n"),
            ("A\n\nC\n".into(), 0)
        );
    }

    #[test]
    fn frontmatter_is_one_block_and_blank_line_runs_separate_paragraphs() {
        assert_eq!(
            paragraphs(
                "---\ntitle: X\n\nstatus: draft\n---\n\nBody one\nline two\n\n\n\nBody two\n"
            ),
            vec![
                "---\ntitle: X\n\nstatus: draft\n---",
                "Body one\nline two",
                "Body two"
            ]
        );
        assert!(paragraphs("").is_empty());
    }
}
