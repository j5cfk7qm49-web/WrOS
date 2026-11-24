//! Stable library editions and content-addressed media.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::model::{Builtin, FieldId, NodeId, OpId};

pub const SOURCE_TAGS: [Builtin; 6] = [
    Builtin::Book,
    Builtin::Article,
    Builtin::Chapter,
    Builtin::Lecture,
    Builtin::Audio,
    Builtin::Web,
];

pub type Bibliography = Map<String, Value>;

pub fn csl_type(kind: Builtin) -> &'static str {
    match kind {
        Builtin::Article => "article-journal",
        Builtin::Chapter => "chapter",
        Builtin::Lecture => "speech",
        Builtin::Audio => "song",
        Builtin::Web => "webpage",
        _ => "book",
    }
}

/// Use the built-in IDs so renamed labels do not change a citation's fields.
pub fn bibliography_for_work(
    kind: Builtin,
    title: &str,
    fields: &[(FieldId, String)],
) -> Bibliography {
    use serde_json::json;
    let mut record = Bibliography::from_iter([
        ("id".into(), json!("source")),
        ("type".into(), json!(csl_type(kind))),
        ("title".into(), json!(title)),
    ]);
    for (field, value) in fields {
        let FieldId::Builtin { tag, slot } = field else {
            continue;
        };
        if *tag != kind {
            continue;
        }
        let name = match (tag, *slot) {
            (
                Builtin::Book
                | Builtin::Article
                | Builtin::Chapter
                | Builtin::Lecture
                | Builtin::Audio
                | Builtin::Web,
                0,
            ) => "author",
            (
                Builtin::Book
                | Builtin::Article
                | Builtin::Chapter
                | Builtin::Lecture
                | Builtin::Audio
                | Builtin::Web,
                1,
            ) => "issued",
            (Builtin::Book, 2) => "ISBN",
            (Builtin::Article, 2) => "DOI",
            (Builtin::Article, 3) | (Builtin::Chapter, 2) | (Builtin::Web, 3) => "container-title",
            (Builtin::Chapter, 3) => "editor",
            (Builtin::Lecture, 2) => "event-title",
            (Builtin::Audio, 2) => "publisher",
            (Builtin::Audio, 3) | (Builtin::Web, 2) => "URL",
            _ => continue,
        };
        let value = match name {
            "author" | "editor" => json!([{"literal":value}]),
            "issued" => {
                use chrono::Datelike;
                if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                    json!({"date-parts": [[date.year(), date.month() as i32, date.day() as i32]]})
                } else if let Ok(year) = value.parse::<i32>() {
                    json!({"date-parts": [[year]]})
                } else {
                    continue;
                }
            }
            _ => json!(value),
        };
        record.insert(name.into(), value);
    }
    record
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EditionKey {
    pub work: NodeId,
    pub edition: OpId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edition {
    pub label: String,
    pub bibliography: Bibliography,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Media>,
}

impl Edition {
    pub fn new(bibliography: Bibliography, added_at: i64) -> Self {
        let label = text(&bibliography, "edition");
        Self {
            label: if label.is_empty() {
                "Edition".into()
            } else {
                label.into()
            },
            bibliography,
            added_at,
            files: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.title().trim().is_empty()
            && !text(&self.bibliography, "type").is_empty()
            && !self.label.trim().is_empty()
            && self.label.len() <= 512
            && self.files.iter().all(Media::is_valid)
            && serde_json::to_vec(&self.bibliography).is_ok_and(|data| data.len() <= 262_144)
    }

    pub fn title(&self) -> &str {
        text(&self.bibliography, "title")
    }

    pub fn source_tag(&self) -> Builtin {
        match text(&self.bibliography, "type") {
            "chapter" | "entry" | "entry-dictionary" | "entry-encyclopedia" => Builtin::Chapter,
            "article" | "article-journal" | "article-magazine" | "article-newspaper"
            | "paper-conference" => Builtin::Article,
            "speech" => Builtin::Lecture,
            "song" | "broadcast" | "motion_picture" => Builtin::Audio,
            "webpage" | "post" | "post-weblog" => Builtin::Web,
            _ => Builtin::Book,
        }
    }

    pub(crate) fn author(&self) -> String {
        self.bibliography
            .get("author")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|author| {
                author["family"]
                    .as_str()
                    .or_else(|| author["literal"].as_str())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn year(&self) -> Option<i64> {
        self.bibliography.get("issued")?["date-parts"][0][0].as_i64()
    }
}

fn text<'a>(record: &'a Bibliography, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub sha256: String,
    pub size: u64,
    pub name: String,
    pub kind: MediaKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Pdf,
    Epub,
    Audio,
}

impl Media {
    pub(crate) fn is_valid(&self) -> bool {
        self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
            && !self.name.is_empty()
            && self.name.len() <= 1024
            && !self
                .name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Title,
    Author,
    Year,
    Type,
    Status,
    Added,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub text: String,
    pub source_type: Option<Builtin>,
    pub tag: Option<NodeId>,
    pub sort: Sort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Work {
    pub id: NodeId,
    pub title: String,
    pub author: String,
    pub year: Option<i64>,
    pub source_type: Builtin,
    pub editions: u32,
    pub status: String,
}

#[cfg(feature = "runtime")]
pub fn import_media(
    library: &std::path::Path,
    input: &std::path::Path,
) -> crate::projection::Result<Media> {
    use std::fs::{self, File};
    use std::io::{self, Seek};

    let name = input
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Choose a file with a readable name")?;
    let kind = match input
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => MediaKind::Pdf,
        "epub" => MediaKind::Epub,
        "mp3" | "m4a" | "m4b" | "aac" | "flac" | "ogg" | "opus" | "wav" => MediaKind::Audio,
        _ => return Err("Choose a PDF, EPUB, or audio file".into()),
    };
    if !input.symlink_metadata()?.is_file() {
        return Err("Choose a regular media file".into());
    }
    let mut source = File::open(input)?;
    let at_open = source.metadata()?;
    let files = library.join("files");
    fs::create_dir_all(&files)?;
    if !files.symlink_metadata()?.is_dir() {
        return Err("Brain files needs a regular directory".into());
    }
    let (digest, size) = hash_media(&mut source, io::sink())?;
    if size != at_open.len() || source.metadata()?.modified()? != at_open.modified()? {
        return Err("The media file changed while reading".into());
    }
    let media = Media {
        sha256: digest,
        size,
        name: name.to_owned(),
        kind,
    };
    if !media.is_valid() {
        return Err("The media file has an unsupported name".into());
    }
    let target = files.join(&media.sha256);
    match target.symlink_metadata() {
        Ok(_) => {
            verify_media(&target, &media)?;
            return Ok(media);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    source.rewind()?;
    let mut pending = tempfile::NamedTempFile::new_in(library)?;
    let (copied_digest, copied_size) = hash_media(&mut source, &mut pending)?;
    if copied_digest != media.sha256 || copied_size != media.size {
        return Err("The media file changed while reading".into());
    }
    pending.as_file().sync_all()?;
    match pending.persist_noclobber(&target) {
        Ok(_) => {
            crate::storage::sync_directory(&files)?;
            crate::storage::sync_directory(library)?;
        }
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            verify_media(&target, &media)?;
        }
        Err(error) => return Err(error.error.into()),
    }
    Ok(media)
}

#[cfg(feature = "runtime")]
fn hash_media(
    source: &mut std::fs::File,
    mut destination: impl std::io::Write,
) -> std::io::Result<(String, u64)> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut size = 0;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        destination.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

#[cfg(feature = "runtime")]
fn verify_media(target: &std::path::Path, media: &Media) -> crate::projection::Result<()> {
    let metadata = target.symlink_metadata()?;
    if !metadata.is_file() || metadata.len() != media.size {
        return Err("The stored media differs from its fingerprint".into());
    }
    let mut existing = std::fs::File::open(target)?;
    let (digest, size) = hash_media(&mut existing, std::io::sink())?;
    if size != media.size || digest != media.sha256 {
        return Err("The stored media differs from its fingerprint".into());
    }
    Ok(())
}
