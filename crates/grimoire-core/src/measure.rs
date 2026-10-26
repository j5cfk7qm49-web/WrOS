//! Operation-based, prose-free source-word accounting. No baseline/Git inference.
//!
//! V1 units are whitespace tokens containing alphanumeric characters, with
//! private notes/TKs and Markdown footnote labels excluded. This is not the
//! native Grimoire whitespace counter or a rendered-Markdown parser. Word units
//! survive character edits; only unmatched units are additions/removals. Whole
//! selection replacement really removes/reinserts units, even at equal length.
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub const MAX_MEASURE_BYTES: usize = 262_144;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Typing,
    Paste,
    Replace,
    Formatting,
    Undo,
    Redo,
    Restore,
    Programmatic,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub added: u64,
    pub removed: u64,
    pub foot_added: u64,
    pub foot_removed: u64,
    pub to_footnote: u64,
    pub from_footnote: u64,
    pub to_excluded: u64,
    pub from_excluded: u64,
    pub foot_to_excluded: u64,
    pub excluded_to_foot: u64,
    pub inserted_chars: u64,
    pub deleted_chars: u64,
    pub operations: u64,
    pub gaps: u64,
}
impl Counts {
    pub fn plus(&mut self, b: Self) {
        macro_rules! add {($($f:ident),*)=>{$(self.$f=self.$f.saturating_add(b.$f);)*}}
        add!(
            added,
            removed,
            foot_added,
            foot_removed,
            to_footnote,
            from_footnote,
            to_excluded,
            from_excluded,
            foot_to_excluded,
            excluded_to_foot,
            inserted_chars,
            deleted_chars,
            operations,
            gaps
        );
    }
    pub fn inverse(self) -> Self {
        Self {
            added: self.removed,
            removed: self.added,
            foot_added: self.foot_removed,
            foot_removed: self.foot_added,
            to_footnote: self.from_footnote,
            from_footnote: self.to_footnote,
            to_excluded: self.from_excluded,
            from_excluded: self.to_excluded,
            foot_to_excluded: self.excluded_to_foot,
            excluded_to_foot: self.foot_to_excluded,
            inserted_chars: self.deleted_chars,
            deleted_chars: self.inserted_chars,
            ..self
        }
    }
    pub fn net(self, footnotes: bool) -> i128 {
        self.added as i128 - self.removed as i128 + self.from_footnote as i128
            - self.to_footnote as i128
            + self.from_excluded as i128
            - self.to_excluded as i128
            + if footnotes {
                self.foot_added as i128 - self.foot_removed as i128 + self.to_footnote as i128
                    - self.from_footnote as i128
                    + self.excluded_to_foot as i128
                    - self.foot_to_excluded as i128
            } else {
                0
            }
    }
    pub fn input(self) -> bool {
        self.inserted_chars > 0 || self.deleted_chars > 0 || self.gaps > 0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mutation {
    pub origin: Origin,
    pub counts: Counts,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Main,
    Foot,
    Excluded,
}
#[derive(Debug)]
struct Word {
    range: Range<usize>,
    letters: Vec<usize>,
    class: Class,
}
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = vec![];
    for range in ranges {
        if let Some(last) = out.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            out.push(range);
        }
    }
    out
}
fn contains(ranges: &[Range<usize>], at: usize) -> bool {
    ranges
        .get(ranges.partition_point(|r| r.end <= at))
        .is_some_and(|r| r.contains(&at))
}
/// Excluded source units are retained only in memory for identity matching.
/// Revealing a private note/TK/reference label is reclassification, never new
/// authored prose. No excluded text is serialized in an operation receipt.
fn words(text: &str) -> Vec<Word> {
    let mut excluded: Vec<_> = crate::notes::ranges(text)
        .into_iter()
        .map(|(a, b, _)| a..b)
        .collect();
    let mut foot_ranges = vec![];
    let mut offset = 0;
    let mut foot = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let definition = trimmed.starts_with("[^") && trimmed.find("]:").is_some();
        if definition {
            foot = true;
        } else if !trimmed.trim().is_empty() && indent < 4 && !line.starts_with('\t') {
            foot = false;
        }
        if foot {
            foot_ranges.push(offset..offset + line.len());
        }
        let mut at = 0;
        while let Some(start) = line[at..].find("[^").map(|n| n + at) {
            let end = line[start..]
                .find(']')
                .map_or(line.trim_end_matches('\n').len(), |n| start + n + 1);
            let end = if definition && start == indent && line[end..].starts_with(':') {
                end + 1
            } else {
                end
            };
            excluded.push(offset + start..offset + end);
            at = end;
            if at >= line.len() {
                break;
            }
        }
        offset += line.len();
    }
    let excluded = merged(excluded);
    let feet = merged(foot_ranges);
    let mut out = vec![];
    let mut start = None;
    let mut letters = vec![];
    let mut current = Class::Main;
    let finish = |end: usize,
                  start: &mut Option<usize>,
                  letters: &mut Vec<usize>,
                  class: Class,
                  out: &mut Vec<Word>| {
        if let Some(a) = start.take()
            && !letters.is_empty()
        {
            out.push(Word {
                range: a..end,
                letters: std::mem::take(letters),
                class,
            });
        }
        letters.clear();
    };
    for (i, c) in text.char_indices() {
        let class = if contains(&excluded, i) {
            Class::Excluded
        } else if contains(&feet, i) {
            Class::Foot
        } else {
            Class::Main
        };
        if c.is_whitespace() {
            finish(i, &mut start, &mut letters, current, &mut out);
            continue;
        }
        if class != current {
            finish(i, &mut start, &mut letters, current, &mut out);
            current = class;
        }
        start.get_or_insert(i);
        if c.is_alphanumeric() {
            letters.push(i);
        }
    }
    finish(text.len(), &mut start, &mut letters, current, &mut out);
    out
}
/// Measure the actual replacement range *before* applying it. Byte positions
/// must be UTF-8 boundaries. No inferred diff is substituted for the operation.
pub fn replacement(before: &str, range: Range<usize>, insert: &str) -> Counts {
    let mut count = Counts {
        inserted_chars: insert.chars().count() as u64,
        deleted_chars: before.get(range.clone()).map_or(0, |s| s.chars().count()) as u64,
        operations: 1,
        ..Counts::default()
    };
    if range.start > range.end
        || before.len() > MAX_MEASURE_BYTES
        || before
            .len()
            .saturating_sub(range.len())
            .saturating_add(insert.len())
            > MAX_MEASURE_BYTES
        || range.end > before.len()
        || !before.is_char_boundary(range.start)
        || !before.is_char_boundary(range.end)
    {
        count.gaps = 1;
        return count;
    }
    if before[range.clone()] == *insert {
        return Counts::default();
    }
    let after = format!(
        "{}{}{}",
        &before[..range.start],
        insert,
        &before[range.end..]
    );
    let old = words(before);
    let new = words(&after);
    // Unchanged characters establish identity. One-to-one matching handles a
    // split/merge without counting every partial-word keystroke as a new word.
    let mut candidates = Vec::with_capacity(old.len());
    for token in &old {
        let mut choices = Vec::new();
        for &pos in &token.letters {
            let mapped = if pos < range.start {
                Some(pos)
            } else if pos >= range.end {
                Some(pos - range.len() + insert.len())
            } else {
                None
            };
            if let Some(mapped) = mapped {
                let n = new.partition_point(|w| w.range.end <= mapped);
                if new.get(n).is_some_and(|w| w.range.contains(&mapped))
                    && choices.last() != Some(&n)
                {
                    choices.push(n);
                }
            }
        }
        candidates.push(choices);
    }
    fn assign(
        old: usize,
        candidates: &[Vec<usize>],
        owners: &mut [Option<usize>],
        seen: &mut [usize],
        epoch: usize,
    ) -> bool {
        for &new in &candidates[old] {
            if seen[new] == epoch {
                continue;
            }
            seen[new] = epoch;
            if owners[new].is_none_or(|previous| assign(previous, candidates, owners, seen, epoch))
            {
                owners[new] = Some(old);
                return true;
            }
        }
        false
    }
    let mut owners = vec![None; new.len()];
    let mut matched = vec![false; old.len()];
    let mut seen = vec![0; new.len()];
    for (i, slot) in matched.iter_mut().enumerate() {
        *slot = assign(i, &candidates, &mut owners, &mut seen, i + 1);
    }
    for (i, w) in old.iter().enumerate() {
        if !matched[i] {
            match w.class {
                Class::Main => count.removed += 1,
                Class::Foot => count.foot_removed += 1,
                Class::Excluded => {}
            }
        }
    }
    for (i, w) in new.iter().enumerate() {
        if owners[i].is_none() {
            match w.class {
                Class::Main => count.added += 1,
                Class::Foot => count.foot_added += 1,
                Class::Excluded => {}
            }
        }
    }
    for (i, owner) in owners.iter().enumerate() {
        if let Some(j) = owner {
            match (old[*j].class, new[i].class) {
                (Class::Main, Class::Foot) => count.to_footnote += 1,
                (Class::Foot, Class::Main) => count.from_footnote += 1,
                (Class::Main, Class::Excluded) => count.to_excluded += 1,
                (Class::Excluded, Class::Main) => count.from_excluded += 1,
                (Class::Foot, Class::Excluded) => count.foot_to_excluded += 1,
                (Class::Excluded, Class::Foot) => count.excluded_to_foot += 1,
                _ => {}
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    fn totals(s: &str) -> (i128, i128) {
        let w = words(s);
        (
            w.iter().filter(|w| w.class == Class::Main).count() as i128,
            w.iter().filter(|w| w.class == Class::Foot).count() as i128,
        )
    }
    #[test]
    fn letter_growth_is_not_one_added_word_per_key() {
        let mut text = String::new();
        let mut counts = Counts::default();
        for c in "hello world".chars() {
            counts.plus(replacement(&text, text.len()..text.len(), &c.to_string()));
            text.push(c);
        }
        assert_eq!((counts.added, counts.removed, counts.net(false)), (2, 0, 2));
    }
    #[test]
    fn equal_count_replacement_counts_both_real_sides() {
        let c = replacement("cat", 0..3, "dog");
        assert_eq!((c.added, c.removed, c.net(false)), (1, 1, 0));
    }
    #[test]
    fn actual_replacement_does_not_infer_shared_suffix_identity() {
        let c = replacement("cat", 0..3, "dog cat");
        assert_eq!((c.added, c.removed), (2, 1));
    }
    #[test]
    fn splitting_and_merging_have_conserved_unit_delta() {
        let c = replacement("helloworld", 5..5, " ");
        assert_eq!((c.added, c.removed), (1, 0));
        let c = replacement("hello world", 5..6, "");
        assert_eq!((c.added, c.removed), (0, 1));
    }
    #[test]
    fn partial_deletion_keeps_the_existing_word() {
        assert_eq!(replacement("hello", 4..5, "").removed, 0);
        assert_eq!(replacement("h", 0..1, "").removed, 1);
    }
    #[test]
    fn punctuation_and_formatting_do_not_reauthor_words() {
        let c = replacement("word", 0..0, "**");
        assert_eq!((c.added, c.removed), (0, 0));
        assert_eq!(replacement("word", 4..4, "!").added, 0);
    }
    #[test]
    fn unicode_boundaries_and_words_are_retained() {
        let s = "café 猫";
        let c = replacement(s, 3..5, "è");
        assert_eq!((c.added, c.removed), (0, 0));
        assert_eq!(replacement(s, 4..5, "x").gaps, 1);
    }
    #[test]
    fn private_notes_tks_and_footnote_labels_are_not_words() {
        assert_eq!(totals("one %% private note %% two TK. [^label]"), (2, 0));
    }
    #[test]
    fn footnotes_have_their_own_units_and_continuations() {
        assert_eq!(
            totals(
                "Body.

[^n]: two words
    more words

ordinary again"
            ),
            (3, 4)
        );
    }
    #[test]
    fn incomplete_footnote_marker_does_not_create_label_words() {
        assert_eq!(totals("body [^longlabel"), (1, 0));
    }
    #[test]
    fn footnote_reclassification_is_not_newly_authored_prose() {
        let c = replacement("words here", 0..0, "[^n]: ");
        assert_eq!(
            (c.added, c.removed, c.to_footnote, c.from_footnote),
            (0, 0, 2, 0)
        );
        assert_eq!(c.net(true), 0);
        assert_eq!(c.net(false), -2);
    }
    #[test]
    fn unchanged_operation_is_not_activity() {
        assert_eq!(replacement("same", 0..4, "same"), Counts::default());
    }
    #[test]
    fn oversize_operations_are_explicit_gaps_not_zero_measurements() {
        let s = "x".repeat(MAX_MEASURE_BYTES + 1);
        assert_eq!(replacement(&s, 0..0, "a").gaps, 1);
    }
    #[test]
    fn inverse_replays_are_separate_exact_deltas() {
        let c = replacement("one two", 0..3, "three four");
        let mut total = c;
        total.plus(c.inverse());
        assert_eq!(total.net(true), 0);
        assert_eq!(c.inverse().inverse(), c);
    }
    #[test]
    fn every_small_utf8_replacement_conserves_both_scoped_net_counts() {
        for before in [
            "",
            "a bb c",
            "a%% note %%b",
            "TK one",
            "a[^n] b",
            "[^n]: one two\n    other",
            "café 猫",
            "**a** b",
        ] {
            let bounds: Vec<_> = before
                .char_indices()
                .map(|(i, _)| i)
                .chain([before.len()])
                .collect();
            for &a in &bounds {
                for &b in bounds.iter().filter(|&&b| b >= a) {
                    for insert in ["", "x", " ", "one two", "[^m]: ", "%%", "é"] {
                        let after = format!("{}{}{}", &before[..a], insert, &before[b..]);
                        let c = replacement(before, a..b, insert);
                        let (bm, bf) = totals(before);
                        let (am, af) = totals(&after);
                        assert_eq!(c.net(false), am - bm, "{before:?} {a}..{b} {insert:?}");
                        assert_eq!(c.net(true), am + af - bm - bf);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod classification_tests {
    use super::*;
    #[test]
    fn revealing_a_private_note_is_transfer_not_authored_additions() {
        let s = "%% hidden words %%";
        let c = replacement(s, 0..2, "");
        assert_eq!(
            (c.added, c.removed, c.from_excluded, c.net(false)),
            (0, 0, 2, 2)
        );
        assert_eq!(c.inverse().net(false), -2);
    }
    #[test]
    fn hiding_existing_prose_is_not_a_deleted_authored_word() {
        let c = replacement("existing prose", 0..0, "%% ");
        assert_eq!(
            (c.removed, c.added, c.to_excluded, c.net(false)),
            (0, 0, 2, -2)
        );
    }
    #[test]
    fn reference_labels_revealed_as_prose_are_not_new_words() {
        let c = replacement("[^label]", 0..2, "");
        assert_eq!((c.added, c.from_excluded, c.net(false)), (0, 1, 1));
    }
    #[test]
    fn private_footnote_transfers_obey_footnote_switch() {
        let s = "[^n]: private words";
        let c = replacement(s, 6..6, "%% ");
        assert_eq!((c.added, c.removed, c.foot_to_excluded), (0, 0, 2));
        assert_eq!(c.net(false), 0);
        assert_eq!(c.net(true), -2);
        assert_eq!(c.inverse().net(true), 2);
    }
    #[test]
    fn private_source_editing_stays_character_only() {
        let s = "%% secret words %%";
        let c = replacement(s, 3..9, "different");
        assert_eq!(
            (c.added, c.removed, c.foot_added, c.foot_removed),
            (0, 0, 0, 0)
        );
        assert_eq!((c.inserted_chars, c.deleted_chars), (9, 6));
    }
}
