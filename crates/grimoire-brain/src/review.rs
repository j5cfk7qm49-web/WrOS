//! Card identity, outline syntax and deterministic review scheduling.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use fsrs::{DEFAULT_PARAMETERS, FSRS, MemoryState};
use serde::{Deserialize, Serialize};

use crate::model::{NodeId, OpId};

const RATING_MAP: u16 = 1;
const RETENTION: f32 = 0.90;
const DAY: i64 = 86_400;
const MAX_INTERVAL_DAYS: u32 = 36_500;
const MAX_CARD_TEXT_BYTES: usize = 16 * 1024;
const MAX_CLOZES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CardId {
    pub created: OpId,
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CardKey {
    pub node: NodeId,
    pub id: CardId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Forward,
    Reverse,
    Cloze,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    kind: Kind,
    pub prompt: String,
    /// An empty forward answer is filled from the note's children.
    pub answer: String,
}

struct Draft {
    kind: Kind,
    prompt: String,
    answer: String,
}

fn draft(kind: Kind, prompt: &str, answer: &str) -> Draft {
    Draft {
        kind,
        prompt: prompt.trim().to_owned(),
        answer: answer.trim().to_owned(),
    }
}

fn drafts(text: &str) -> Vec<Draft> {
    for (separator, reverse) in [(" :::", true), (" ::", false)] {
        if let Some((front, back)) = text.split_once(separator)
            && !front.trim().is_empty()
            && (back.is_empty() || back.starts_with(char::is_whitespace))
        {
            let mut items = vec![draft(Kind::Forward, front, back)];
            if reverse && !back.trim().is_empty() {
                items.push(draft(Kind::Reverse, back, front));
            }
            return items;
        }
    }

    let mut blanks = Vec::new();
    let mut rest = text;
    while let Some((before, opening)) = rest.split_once("{{") {
        let Some((answer, after)) = opening.split_once("}}") else {
            break;
        };
        let start = text.len() - rest.len() + before.len();
        let end = text.len() - after.len();
        if !answer.trim().is_empty() && !answer.contains("{{") {
            blanks.push((start, end, answer));
            if blanks.len() == MAX_CLOZES {
                break;
            }
        }
        rest = after;
    }
    blanks
        .iter()
        .enumerate()
        .map(|(hidden, &(_, _, answer))| {
            let mut prompt = String::new();
            let mut cursor = 0;
            for (index, &(start, end, text_value)) in blanks.iter().enumerate() {
                prompt.push_str(&text[cursor..start]);
                prompt.push_str(if index == hidden { "[…]" } else { text_value });
                cursor = end;
            }
            prompt.push_str(&text[cursor..]);
            draft(Kind::Cloze, &prompt, answer)
        })
        .collect()
}

/// Preserve exact cloze answers across moves. Pair the remaining slots in
/// outline order when a kind retains the same number of unmatched cards.
pub fn cards_for_text(text: &str, old: &[Card], edit: OpId) -> Vec<Card> {
    if !looks_like_card(text) {
        return Vec::new();
    }
    let drafts = drafts(text);
    let mut assigned = vec![None; drafts.len()];
    let mut used = vec![false; old.len()];
    for (position, item) in drafts.iter().enumerate() {
        if let Some(index) = old.iter().enumerate().position(|(i, card)| {
            !used[i]
                && card.kind == item.kind
                && card.answer == item.answer
                && (item.kind == Kind::Cloze || card.prompt == item.prompt)
        }) {
            assigned[position] = Some(old[index].id);
            used[index] = true;
        }
    }
    for kind in [Kind::Forward, Kind::Reverse, Kind::Cloze] {
        let previous: Vec<_> = old
            .iter()
            .zip(&used)
            .filter_map(|(card, used)| (!used && card.kind == kind).then_some(card.id))
            .collect();
        let pending: Vec<_> = drafts
            .iter()
            .zip(&mut assigned)
            .filter_map(|(item, id)| (id.is_none() && item.kind == kind).then_some(id))
            .collect();
        if previous.len() == pending.len() {
            for (id, existing) in pending.into_iter().zip(previous) {
                *id = Some(existing);
            }
        }
    }
    drafts
        .into_iter()
        .zip(assigned)
        .enumerate()
        .map(|(slot, (item, id))| Card {
            id: id.unwrap_or(CardId {
                created: edit,
                slot: slot as u32,
            }),
            kind: item.kind,
            prompt: item.prompt,
            answer: item.answer,
        })
        .collect()
}

pub fn looks_like_card(text: &str) -> bool {
    text.len() <= MAX_CARD_TEXT_BYTES && (text.contains("{{") || text.contains(" ::"))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Review {
    pub card: CardKey,
    pub grade: u8,
    pub rating_map: u16,
    /// The wall time and offset at the point of review preserve the local day.
    pub observed_at: DateTime<FixedOffset>,
    /// UTC seconds. A device clamps backwards clock jumps per card at capture.
    pub effective_at: i64,
}

impl PartialEq for Review {
    fn eq(&self, other: &Self) -> bool {
        self.card == other.card
            && self.grade == other.grade
            && self.rating_map == other.rating_map
            && self.observed_at == other.observed_at
            && self.observed_at.offset() == other.observed_at.offset()
            && self.effective_at == other.effective_at
    }
}

impl Eq for Review {}

impl Review {
    pub fn capture(
        card: CardKey,
        grade: u8,
        observed_at: DateTime<FixedOffset>,
        previous_on_device: Option<i64>,
    ) -> Result<Self, ReviewError> {
        let event = Self {
            card,
            grade,
            rating_map: RATING_MAP,
            observed_at,
            effective_at: observed_at
                .timestamp()
                .max(previous_on_device.unwrap_or(i64::MIN)),
        };
        event.rating()?;
        Ok(event)
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.rating().is_ok()
    }

    fn rating(&self) -> Result<u8, ReviewError> {
        if self.rating_map != RATING_MAP {
            return Err(ReviewError::UnknownMapping(self.rating_map));
        }
        if self.effective_at < self.observed_at.timestamp()
            || DateTime::from_timestamp(self.effective_at, 0).is_none()
        {
            return Err(ReviewError::InvalidTime);
        }
        [1, 1, 2, 3, 3, 4]
            .get(usize::from(self.grade))
            .copied()
            .ok_or(ReviewError::InvalidGrade)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("A review grade must be 0 through 5")]
    InvalidGrade,
    #[error("Invalid review timestamp")]
    InvalidTime,
    #[error("Unknown review rating map {0}")]
    UnknownMapping(u16),
    #[error("Conflicting review events have the same identity")]
    ConflictingId,
    #[error("Review events belong to different cards")]
    MixedCards,
    #[error(transparent)]
    Scheduling(#[from] fsrs::FSRSError),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub due_at: i64,
    pub reviews: u32,
    pub(crate) last: Review,
    pub(crate) stability: f32,
    pub(crate) difficulty: f32,
}

impl Schedule {
    /// The event ID is the durable operation ID and breaks timestamp ties.
    pub fn replay<'a>(
        events: impl IntoIterator<Item = (OpId, &'a Review)>,
    ) -> Result<Option<Self>, ReviewError> {
        let mut by_id = BTreeMap::new();
        for (id, event) in events {
            if by_id.insert(id, event).is_some_and(|old| old != event) {
                return Err(ReviewError::ConflictingId);
            }
        }
        let mut events: Vec<_> = by_id.into_iter().collect();
        events.sort_unstable_by_key(|(id, event)| (event.effective_at, *id));
        let fsrs = FSRS::new(&DEFAULT_PARAMETERS)?;
        events.into_iter().try_fold(None, |state, (_, event)| {
            advance(&fsrs, state, event).map(Some)
        })
    }
}

pub fn update_schedule(
    previous: Option<Schedule>,
    event: &Review,
) -> Result<Schedule, ReviewError> {
    advance(&FSRS::new(&DEFAULT_PARAMETERS)?, previous, event)
}

fn advance(
    fsrs: &FSRS,
    previous: Option<Schedule>,
    event: &Review,
) -> Result<Schedule, ReviewError> {
    let rating = event.rating()?;
    if previous.is_some_and(|state| state.last.card != event.card) {
        return Err(ReviewError::MixedCards);
    }
    let elapsed = previous.map_or(0, |state| {
        let local_days =
            (event.observed_at.date_naive() - state.last.observed_at.date_naive()).num_days();
        let utc_days = event.effective_at.div_euclid(DAY) - state.last.effective_at.div_euclid(DAY);
        local_days.clamp(0, (utc_days + 1).clamp(0, i64::from(MAX_INTERVAL_DAYS))) as u32
    });
    let memory = previous.map(|state| MemoryState {
        stability: state.stability,
        difficulty: state.difficulty,
    });
    let states = fsrs.next_states(memory, RETENTION, elapsed)?;
    let state = match rating {
        1 => states.again,
        2 => states.hard,
        3 => states.good,
        _ => states.easy,
    };
    let seconds = (f64::from(state.interval) * DAY as f64)
        .round()
        .clamp(60.0, f64::from(MAX_INTERVAL_DAYS) * DAY as f64) as i64;
    Ok(Schedule {
        due_at: event.effective_at.saturating_add(seconds),
        reviews: previous.map_or(1, |state| state.reviews.saturating_add(1)),
        last: *event,
        stability: state.memory.stability,
        difficulty: state.memory.difficulty,
    })
}
