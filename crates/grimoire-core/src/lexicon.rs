//! Read-only WordNet 3.0 index/data reader. No model, network, generated
//! definitions or manuscript storage. Senses and *explicit* antonym pointers
//! stay separate; a synonym is never inferred to be interchangeable in context.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const DEFAULT_ROOT: &str = "/usr/share/wordnet";
pub const VERSION: &str = "WordNet 3.0";
pub const MAX_SENSES: usize = 64;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 65_536;
const MAX_ANTONYMS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PartOfSpeech {
    Noun,
    Verb,
    Adjective,
    Adverb,
}
impl PartOfSpeech {
    pub const ALL: [Self; 4] = [Self::Noun, Self::Verb, Self::Adjective, Self::Adverb];
    pub fn name(self) -> &'static str {
        match self {
            Self::Noun => "noun",
            Self::Verb => "verb",
            Self::Adjective => "adjective",
            Self::Adverb => "adverb",
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Noun => "noun",
            Self::Verb => "verb",
            Self::Adjective => "adj",
            Self::Adverb => "adv",
        }
    }
    fn from_code(code: &str) -> Result<Self> {
        match code {
            "n" => Ok(Self::Noun),
            "v" => Ok(Self::Verb),
            "a" | "s" => Ok(Self::Adjective),
            "r" => Ok(Self::Adverb),
            _ => bail!("unsupported WordNet part of speech"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SenseId {
    pub part: PartOfSpeech,
    /// Byte offset in the corresponding data file, NOT a character offset.
    pub offset: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Antonym {
    pub word: String,
    pub target: SenseId,
    pub gloss: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sense {
    pub id: SenseId,
    pub lemma: String,
    /// Exact index sense order for this lemma/POS; not a quality ranking.
    pub number: usize,
    pub morphological: bool,
    pub words: Vec<String>,
    /// Includes upstream usage examples when present; never invented.
    pub gloss: String,
    pub antonyms: Vec<Antonym>,
    pub adjective_satellite: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lookup {
    pub query: String,
    pub version: String,
    pub senses: Vec<Sense>,
    pub truncated: bool,
}
#[derive(Debug, Clone)]
pub struct Dictionary {
    root: PathBuf,
}
struct Synset {
    id: SenseId,
    words: Vec<String>,
    gloss: String,
    pointers: Vec<Pointer>,
    satellite: bool,
}
struct Pointer {
    target: SenseId,
    source_word: usize,
    target_word: usize,
}

pub fn normalize_query(query: &str) -> Result<String> {
    ensure!(
        !query.chars().any(char::is_control),
        "dictionary query contains a control character"
    );
    ensure!(
        query.chars().count() <= 96,
        "dictionary query exceeds 96 characters"
    );
    let query = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .replace('’', "'")
        .to_lowercase();
    ensure!(!query.is_empty(), "enter a word or short phrase");
    ensure!(
        query.chars().all(|c| c.is_alphabetic()
            || c.is_ascii_digit()
            || unicode_normalization::char::is_combining_mark(c)
            || matches!(c, '_' | '-' | '\'' | '.')),
        "dictionary query contains a path/control or unsupported character"
    );
    Ok(query)
}
impl Dictionary {
    /// Only fixed resource filenames are opened. Constructor and lookup are
    /// blocking I/O and belong on a worker, never in interactive draw/input.
    pub fn open(root: &Path) -> Result<Self> {
        ensure!(
            root.is_dir(),
            "WordNet data unavailable; install wordnet-base or choose an explicit local data root"
        );
        for part in PartOfSpeech::ALL {
            for prefix in ["index", "data"] {
                let path = root.join(format!("{prefix}.{}", part.suffix()));
                let metadata = fs::metadata(&path)
                    .with_context(|| format!("WordNet file unavailable: {}", path.display()))?;
                ensure!(
                    metadata.is_file() && metadata.len() <= MAX_FILE_BYTES,
                    "WordNet resource is not a bounded regular file"
                );
                let mut header = Vec::new();
                File::open(&path)?.take(4096).read_to_end(&mut header)?;
                ensure!(
                    String::from_utf8_lossy(&header).contains(VERSION),
                    "unsupported WordNet data version; expected 3.0 (no download or replacement attempted)"
                );
            }
        }
        for part in PartOfSpeech::ALL {
            let metadata = fs::metadata(root.join(format!("{}.exc", part.suffix())))
                .context("WordNet morphology exceptions unavailable; data set is incomplete")?;
            ensure!(
                metadata.is_file() && metadata.len() <= MAX_FILE_BYTES,
                "WordNet exception resource is not a bounded regular file"
            );
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }
    pub fn lookup(&self, input: &str) -> Result<Lookup> {
        let query = normalize_query(input)?;
        let mut result = Lookup {
            query: input.trim().into(),
            version: VERSION.into(),
            senses: vec![],
            truncated: false,
        };
        let mut seen = BTreeSet::new();
        for part in PartOfSpeech::ALL {
            for lemma in self.candidates(&query, part)? {
                let Some(offsets) = self.offsets(&lemma, part)? else {
                    continue;
                };
                for (number, offset) in offsets.into_iter().enumerate() {
                    let id = SenseId { part, offset };
                    if !seen.insert((lemma.clone(), id.clone())) {
                        continue;
                    }
                    if result.senses.len() >= MAX_SENSES {
                        result.truncated = true;
                        return Ok(result);
                    }
                    let synset = self.synset(id)?;
                    let source_words: BTreeSet<_> = synset
                        .words
                        .iter()
                        .enumerate()
                        .filter(|(_, word)| word.to_lowercase() == lemma)
                        .map(|(i, _)| i + 1)
                        .collect();
                    ensure!(
                        !source_words.is_empty(),
                        "WordNet index points to a synset without the requested lemma"
                    );
                    let mut antonyms = vec![];
                    for pointer in &synset.pointers {
                        if pointer.source_word != 0 && !source_words.contains(&pointer.source_word)
                        {
                            continue;
                        }
                        let target = self.synset(pointer.target.clone())?;
                        ensure!(
                            pointer.target_word <= target.words.len(),
                            "WordNet antonym target word is out of range"
                        );
                        for (index, word) in target.words.iter().enumerate() {
                            if pointer.target_word != 0 && pointer.target_word != index + 1 {
                                continue;
                            }
                            if !antonyms.iter().any(|a: &Antonym| {
                                a.word == display_word(word) && a.target == target.id
                            }) {
                                antonyms.push(Antonym {
                                    word: display_word(word),
                                    target: target.id.clone(),
                                    gloss: target.gloss.clone(),
                                });
                            }
                            if antonyms.len() >= MAX_ANTONYMS {
                                break;
                            }
                        }
                        if antonyms.len() >= MAX_ANTONYMS {
                            break;
                        }
                    }
                    result.senses.push(Sense {
                        id: synset.id,
                        lemma: display_word(&lemma),
                        number: number + 1,
                        morphological: lemma != query,
                        words: synset.words.iter().map(|w| display_word(w)).collect(),
                        gloss: synset.gloss,
                        antonyms,
                        adjective_satellite: synset.satellite,
                    });
                }
            }
        }
        Ok(result)
    }
    fn candidates(&self, query: &str, part: PartOfSpeech) -> Result<Vec<String>> {
        let mut values = vec![query.to_owned()];
        let exception = self.root.join(format!("{}.exc", part.suffix()));
        if exception.exists() {
            ensure!(
                fs::metadata(&exception)?.len() <= MAX_FILE_BYTES,
                "WordNet exception file too large"
            );
            if let Some(line) = matching_line(&exception, query)? {
                for word in line.split_whitespace().skip(1).take(8) {
                    push_unique(&mut values, word.into());
                }
            }
        }
        // WordNet's documented suffix candidates, verified against the index,
        // never silently substituted into a manuscript or invented as senses.
        let suffixes: &[(&str, &str)] = match part {
            PartOfSpeech::Noun => &[
                ("s", ""),
                ("ses", "s"),
                ("xes", "x"),
                ("zes", "z"),
                ("ches", "ch"),
                ("shes", "sh"),
                ("men", "man"),
                ("ies", "y"),
            ],
            PartOfSpeech::Verb => &[
                ("s", ""),
                ("ies", "y"),
                ("es", "e"),
                ("es", ""),
                ("ed", "e"),
                ("ed", ""),
                ("ing", "e"),
                ("ing", ""),
            ],
            PartOfSpeech::Adjective => &[("er", ""), ("est", ""), ("er", "e"), ("est", "e")],
            PartOfSpeech::Adverb => &[],
        };
        for (suffix, replacement) in suffixes {
            if let Some(stem) = query.strip_suffix(suffix).filter(|s| !s.is_empty()) {
                push_unique(&mut values, format!("{stem}{replacement}"));
            }
        }
        Ok(values)
    }
    fn offsets(&self, lemma: &str, part: PartOfSpeech) -> Result<Option<Vec<u64>>> {
        let path = self.root.join(format!("index.{}", part.suffix()));
        let Some(line) = matching_line(&path, lemma)? else {
            return Ok(None);
        };
        let fields: Vec<_> = line.split_whitespace().collect();
        ensure!(
            fields.len() >= 6 && PartOfSpeech::from_code(fields[1])? == part,
            "invalid WordNet index/POS"
        );
        let count = decimal(fields[2])?;
        let pointers = decimal(fields[3])?;
        ensure!(
            count <= 4096 && pointers <= 128 && fields.len() == 6 + pointers + count,
            "invalid WordNet index counts"
        );
        ensure!(
            decimal(fields[4 + pointers])? == count,
            "inconsistent WordNet sense count"
        );
        fields[6 + pointers..]
            .iter()
            .map(|field| field.parse::<u64>().map_err(Into::into))
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }
    fn synset(&self, id: SenseId) -> Result<Synset> {
        let path = self.root.join(format!("data.{}", id.part.suffix()));
        let mut file = File::open(&path)?;
        ensure!(
            id.offset < file.metadata()?.len(),
            "WordNet synset offset outside data file"
        );
        file.seek(SeekFrom::Start(id.offset))?;
        let mut reader = BufReader::new(file);
        let line = limited_line(&mut reader)?.context("missing WordNet synset")?;
        parse_synset(&line, id)
    }
}
fn decimal(value: &str) -> Result<usize> {
    value.parse().context("invalid WordNet decimal field")
}
fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value)
    }
}
fn display_word(word: &str) -> String {
    word.replace('_', " ")
}
fn strip_adjective_marker(word: &str) -> &str {
    for marker in ["(a)", "(p)", "(ip)"] {
        if let Some(base) = word.strip_suffix(marker) {
            return base;
        }
    }
    word
}
fn limited_line(reader: &mut impl BufRead) -> Result<Option<String>> {
    let mut bytes = vec![];
    let length = reader
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut bytes)?;
    if length == 0 {
        return Ok(None);
    }
    ensure!(
        length <= MAX_LINE_BYTES,
        "WordNet record exceeds bounded line limit"
    );
    let text = String::from_utf8(bytes).context("WordNet record is not UTF-8/ASCII")?;
    ensure!(
        !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "WordNet record contains control characters"
    );
    Ok(Some(text))
}
fn matching_line(path: &Path, lemma: &str) -> Result<Option<String>> {
    let mut reader = BufReader::new(File::open(path)?);
    while let Some(line) = limited_line(&mut reader)? {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if line.split_whitespace().next() == Some(lemma) {
            return Ok(Some(line));
        }
    }
    Ok(None)
}
fn parse_synset(line: &str, id: SenseId) -> Result<Synset> {
    let (data, gloss) = line
        .split_once('|')
        .context("WordNet synset has no gloss separator")?;
    let fields: Vec<_> = data.split_whitespace().collect();
    ensure!(
        fields.len() >= 5
            && fields[0].parse::<u64>()? == id.offset
            && PartOfSpeech::from_code(fields[2])? == id.part,
        "WordNet synset offset/POS mismatch"
    );
    let count = usize::from_str_radix(fields[3], 16)?;
    ensure!(
        count > 0 && count <= 256 && fields.len() > 4 + count * 2,
        "invalid WordNet word count"
    );
    let mut words = vec![];
    for pair in fields[4..4 + count * 2].as_chunks::<2>().0 {
        let _: u8 = u8::from_str_radix(pair[1], 16)?;
        words.push(strip_adjective_marker(pair[0]).to_owned());
    }
    let start = 4 + count * 2;
    let pointer_count = decimal(fields[start])?;
    ensure!(
        pointer_count <= 2048 && fields.len() > start + pointer_count * 4,
        "invalid WordNet pointer count"
    );
    let mut pointers = vec![];
    for group in fields[start + 1..start + 1 + pointer_count * 4]
        .as_chunks::<4>()
        .0
    {
        let target = SenseId {
            offset: group[1].parse()?,
            part: PartOfSpeech::from_code(group[2])?,
        };
        let lexical = u16::from_str_radix(group[3], 16)?;
        let source_word = (lexical >> 8) as usize;
        ensure!(
            source_word <= words.len(),
            "WordNet pointer source word outside synset"
        );
        if group[0] == "!" {
            pointers.push(Pointer {
                target,
                source_word,
                target_word: (lexical & 255) as usize,
            });
        }
    }
    let gloss = gloss.trim().to_owned();
    ensure!(!gloss.is_empty(), "empty WordNet gloss");
    Ok(Synset {
        id,
        words,
        gloss,
        pointers,
        satellite: fields[2] == "s",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    fn root() -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "wd-wordnet-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    fn fixture() -> PathBuf {
        let root = root();
        for part in PartOfSpeech::ALL {
            fs::write(root.join(format!("{}.exc", part.suffix())), "").unwrap();
            fs::write(
                root.join(format!("index.{}", part.suffix())),
                "  WordNet 3.0 synthetic format fixture\n",
            )
            .unwrap();
            fs::write(
                root.join(format!("data.{}", part.suffix())),
                "  WordNet 3.0 synthetic format fixture\n",
            )
            .unwrap();
        }
        let header = "  WordNet 3.0 synthetic format fixture\n";
        let a = header.len();
        let first = format!(
            "{a:08} 00 a 02 hot 0 warm 0 1 ! 00000000 a 0101 | a deliberately hot test fixture\n"
        );
        let b = a + first.len();
        let first = first.replacen("! 00000000", &format!("! {b:08}"), 1);
        let second =
            format!("{b:08} 00 a 01 cold 0 1 ! {a:08} a 0101 | a deliberately cold test fixture\n");
        fs::write(root.join("data.adj"), format!("{header}{first}{second}")).unwrap();
        fs::write(
            root.join("index.adj"),
            format!(
                "{header}hot a 1 1 ! 1 0 {a:08}\nwarm a 1 0 1 0 {a:08}\ncold a 1 1 ! 1 0 {b:08}\n"
            ),
        )
        .unwrap();
        root
    }
    #[test]
    fn query_is_a_bounded_lemma_not_a_path_shell_or_normalized_manuscript() {
        assert_eq!(normalize_query("  Good   Faith ").unwrap(), "good_faith");
        assert_eq!(normalize_query("Writer’s").unwrap(), "writer's");
        for bad in ["", "../data.noun", "x\0y", "$(rm)", &"x".repeat(97)] {
            assert!(normalize_query(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn actual_lexical_pointer_not_all_synonyms_grants_a_sense_specific_antonym() {
        let root = fixture();
        let dictionary = Dictionary::open(&root).unwrap();
        let result = dictionary.lookup("HOT").unwrap();
        assert_eq!(result.senses.len(), 1);
        assert_eq!(result.senses[0].words, vec!["hot", "warm"]);
        assert_eq!(result.senses[0].antonyms[0].word, "cold");
        assert!(
            dictionary.lookup("warm").unwrap().senses[0]
                .antonyms
                .is_empty(),
            "pointer source is hot, not its synonym warm"
        );
        assert!(
            dictionary
                .lookup("a_completely_unknown_lemma")
                .unwrap()
                .senses
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn exception_candidates_are_explicit_index_verified_lemmas() {
        let root = fixture();
        fs::write(root.join("adj.exc"), "hotter hot\n").unwrap();
        let result = Dictionary::open(&root).unwrap().lookup("hotter").unwrap();
        assert_eq!(result.senses[0].lemma, "hot");
        assert!(result.senses[0].morphological);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_unsupported_and_corrupt_offsets_are_errors_not_clear_results() {
        let empty = root();
        assert!(Dictionary::open(&empty).is_err());
        fs::remove_dir_all(empty).unwrap();
        let root = fixture();
        fs::write(root.join("data.verb"), "WordNet 4.0\n").unwrap();
        assert!(Dictionary::open(&root).is_err());
        fs::write(root.join("data.verb"), "WordNet 3.0\n").unwrap();
        fs::write(
            root.join("index.adj"),
            "WordNet 3.0\nhot a 1 0 1 0 999999999\n",
        )
        .unwrap();
        assert!(Dictionary::open(&root).unwrap().lookup("hot").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_lines_bad_pos_counts_and_pointer_indices_are_refused() {
        let id = SenseId {
            part: PartOfSpeech::Adjective,
            offset: 100,
        };
        for bad in [
            "00000100 00 z 01 hot 0 0 | bad",
            "00000101 00 a 01 hot 0 0 | shifted",
            "00000100 00 a 01 hot 0 1 ! 00000200 a 0201 | invalid lexical source",
            "00000100 00 a FF hot 0 0 | count",
        ] {
            assert!(parse_synset(bad, id.clone()).is_err());
        }
        let bytes = vec![b'x'; MAX_LINE_BYTES + 1];
        assert!(limited_line(&mut &bytes[..]).is_err());
    }
    #[test]
    fn adjective_satellites_and_position_markers_keep_original_sense_identity() {
        let id = SenseId {
            part: PartOfSpeech::Adjective,
            offset: 100,
        };
        let sense = parse_synset(
            "00000100 00 s 01 ready(p) 0 0 | a synthetic predicative example",
            id.clone(),
        )
        .unwrap();
        assert!(sense.satellite);
        assert_eq!(sense.id, id);
        assert_eq!(sense.words, vec!["ready"]);
    }
}
