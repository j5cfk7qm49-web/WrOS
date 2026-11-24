//! Stable names for exported IDs, field definitions, and bounded saved queries.

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::model::{Builtin, FieldId, NodeId, OpId};

impl Builtin {
    pub const ALL: [Self; 8] = [
        Self::Book,
        Self::Article,
        Self::Chapter,
        Self::Lecture,
        Self::Audio,
        Self::Web,
        Self::Concept,
        Self::Person,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Article => "article",
            Self::Chapter => "chapter",
            Self::Lecture => "lecture",
            Self::Audio => "audio",
            Self::Web => "web",
            Self::Concept => "concept",
            Self::Person => "person",
        }
    }
}

impl NodeId {
    pub fn external_id(self) -> String {
        match self {
            Self::Note(OpId { clock, device }) => format!("n-{clock:016x}-{device:016x}"),
            Self::Today(day) => format!("d-{day}"),
            Self::Builtin(tag) => format!("b-{}", tag.name()),
        }
    }
}

impl FromStr for NodeId {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        const INVALID: &str = "Invalid Brain identifier";
        let (prefix, rest) = raw.split_once('-').ok_or(INVALID)?;
        match prefix {
            "n" => {
                let (clock, device) = rest.split_once('-').ok_or(INVALID)?;
                if clock.len() != 16 || device.len() != 16 {
                    return Err(INVALID);
                }
                Ok(Self::Note(OpId {
                    clock: u64::from_str_radix(clock, 16).map_err(|_| INVALID)?,
                    device: u64::from_str_radix(device, 16).map_err(|_| INVALID)?,
                }))
            }
            "d" => rest.parse().map(Self::Today).map_err(|_| INVALID),
            "b" => Builtin::ALL
                .into_iter()
                .find(|tag| tag.name() == rest)
                .map(Self::Builtin)
                .ok_or(INVALID),
            _ => Err(INVALID),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Link,
    Url,
    Choice(Vec<String>),
    File,
}

impl FieldKind {
    pub fn accepts(&self, value: &str) -> bool {
        match self {
            Self::Text => true,
            Self::Number => value.parse::<f64>().is_ok_and(f64::is_finite),
            Self::Date => value.parse::<NaiveDate>().is_ok(),
            Self::Link => value.parse::<NodeId>().is_ok(),
            Self::Url => value.split_once("://").is_some_and(|(scheme, address)| {
                matches!(scheme, "https" | "http")
                    && !address.split('/').next().unwrap_or("").is_empty()
                    && !address.contains(char::is_whitespace)
            }),
            Self::Choice(choices) => choices.iter().any(|choice| choice == value),
            Self::File => {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_digit() || matches!(c, b'a'..=b'f'))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FieldSpec {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SavedQuery {
    Text(String),
    Tagged(NodeId),
    Field { field: FieldId, value: String },
    LinksTo(NodeId),
}

/// Indices in each list are permanent field IDs. New fields can be appended.
/// Source export can map the IDs independently of a user's labels.
pub fn default_fields(tag: NodeId) -> BTreeMap<FieldId, FieldSpec> {
    use FieldKind::{Date, Number, Text, Url};
    let NodeId::Builtin(tag) = tag else {
        return BTreeMap::new();
    };
    let fields = match tag {
        Builtin::Book => vec![
            ("Author", Text),
            ("Year", Number),
            ("ISBN", Text),
            ("Status", Text),
        ],
        Builtin::Article => vec![
            ("Author", Text),
            ("Year", Number),
            ("DOI", Text),
            ("Journal", Text),
        ],
        Builtin::Chapter => vec![
            ("Author", Text),
            ("Year", Number),
            ("Book title", Text),
            ("Editor", Text),
        ],
        Builtin::Lecture => vec![("Speaker", Text), ("Date", Date), ("Event", Text)],
        Builtin::Audio => vec![
            ("Creator", Text),
            ("Date", Date),
            ("Publisher", Text),
            ("URL", Url),
        ],
        Builtin::Web => vec![
            ("Author", Text),
            ("Date", Date),
            ("URL", Url),
            ("Site", Text),
        ],
        Builtin::Concept => vec![],
        Builtin::Person => vec![("Name", Text), ("URL", Url)],
    };
    fields
        .into_iter()
        .enumerate()
        .map(|(slot, (name, kind))| {
            (
                FieldId::Builtin {
                    tag,
                    slot: slot as u16,
                },
                FieldSpec {
                    name: name.into(),
                    kind,
                    required: false,
                },
            )
        })
        .collect()
}
