//! Read-aloud state model, multi-scene queue and Unicode sentence spans.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceSpan {
    pub index: usize,
    pub range: Range<usize>,
    pub text: String,
}

/// Segment text into sentences with byte offsets for live visual highlighting.
pub fn segment_sentences(text: &str) -> Vec<SentenceSpan> {
    let mut spans: Vec<SentenceSpan> = Vec::new();
    for (offset, sentence) in text.split_sentence_bound_indices() {
        let left = sentence.len() - sentence.trim_start().len();
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            continue;
        }
        let start = offset + left;
        // UAX #29 splits after quoted exclamations. For fiction, keep a
        // following lowercase dialogue tag with that quotation. Offsets still
        // come from the maintained splitter and refer to the original source.
        let dialogue_tag = trimmed.chars().next().is_some_and(char::is_lowercase)
            && spans.last().is_some_and(|previous| {
                let without_quotes = previous.text.trim_end_matches(['"', '\'', '”', '’']);
                without_quotes.len() < previous.text.len()
                    && without_quotes.ends_with(['.', '!', '?', '…'])
            });
        if dialogue_tag {
            let previous = spans.last_mut().unwrap();
            previous.range.end = start + trimmed.len();
            previous.text = text[previous.range.clone()].into();
            continue;
        }
        spans.push(SentenceSpan {
            index: spans.len(),
            range: start..start + trimmed.len(),
            text: trimmed.into(),
        });
    }
    spans
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrationStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedScene {
    pub scene_id: usize,
    pub scene_title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NarrationSession {
    pub scene_id: usize,
    pub scene_title: String,
    pub current_sentence: usize,
    pub status: NarrationStatus,
    pub rate: f32, // 1.0 = normal speed
    pub pitch: f32,
    pub voice: String,
    pub queue: Vec<QueuedScene>,
    pub queue_index: usize,
}

impl NarrationSession {
    pub fn new(scene_id: usize, scene_title: impl Into<String>) -> Self {
        let title = scene_title.into();
        Self {
            scene_id,
            scene_title: title.clone(),
            current_sentence: 0,
            status: NarrationStatus::Stopped,
            rate: 1.0,
            pitch: 1.0,
            voice: "default".into(),
            queue: vec![QueuedScene {
                scene_id,
                scene_title: title,
            }],
            queue_index: 0,
        }
    }

    pub fn with_queue(queue: Vec<QueuedScene>) -> Option<Self> {
        let first = queue.first()?.clone();
        Some(Self {
            scene_id: first.scene_id,
            scene_title: first.scene_title,
            current_sentence: 0,
            status: NarrationStatus::Stopped,
            rate: 1.0,
            pitch: 1.0,
            voice: "default".into(),
            queue,
            queue_index: 0,
        })
    }

    pub fn play(&mut self) {
        self.status = NarrationStatus::Playing;
    }

    pub fn pause(&mut self) {
        self.status = NarrationStatus::Paused;
    }

    pub fn stop(&mut self) {
        self.status = NarrationStatus::Stopped;
        self.current_sentence = 0;
    }

    pub fn next_sentence(&mut self, total_sentences: usize) -> bool {
        if let Some(next) = self
            .current_sentence
            .checked_add(1)
            .filter(|&next| next < total_sentences)
        {
            self.current_sentence = next;
            true
        } else {
            self.stop();
            false
        }
    }

    pub fn advance_scene(&mut self) -> Option<&QueuedScene> {
        let next = self.queue_index.checked_add(1)?;
        let item = self.queue.get(next)?;
        self.queue_index = next;
        self.scene_id = item.scene_id;
        self.scene_title = item.scene_title.clone();
        self.current_sentence = 0;
        self.status = NarrationStatus::Playing;
        self.queue.get(self.queue_index)
    }

    pub fn prev_sentence(&mut self) -> bool {
        if self.current_sentence > 0 {
            self.current_sentence -= 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_spans_are_exact_original_slices_and_keep_lowercase_dialogue_tags() {
        let text = "  “Wait!” she called.\n\nλ returned。次の文！  ";
        let spans = segment_sentences(text);
        assert_eq!(spans[0].text, "“Wait!” she called.");
        assert!(spans.len() >= 3);
        for (i, span) in spans.iter().enumerate() {
            assert_eq!(span.index, i);
            assert_eq!(&text[span.range.clone()], span.text);
            assert_eq!(span.text.trim(), span.text);
        }
        let mut session = NarrationSession::new(1, "Scene");
        session.current_sentence = usize::MAX;
        assert!(!session.next_sentence(1));
        assert_eq!(session.status, NarrationStatus::Stopped);
    }

    #[test]
    fn multi_scene_queue_advances_cleanly() {
        let mut session = NarrationSession::with_queue(vec![
            QueuedScene {
                scene_id: 1,
                scene_title: "One".into(),
            },
            QueuedScene {
                scene_id: 2,
                scene_title: "Two".into(),
            },
        ])
        .unwrap();
        session.play();
        assert!(!session.next_sentence(1));
        let next = session.advance_scene().unwrap();
        assert_eq!(next.scene_id, 2);
        assert_eq!(session.current_sentence, 0);
        assert!(session.advance_scene().is_none());
    }

    #[test]
    fn segment_splits_plain_sentences() {
        let text = "First sentence here. Second sentence follows! Third one?";
        let spans = segment_sentences(text);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "First sentence here.");
        assert_eq!(spans[1].text, "Second sentence follows!");
        assert_eq!(spans[2].text, "Third one?");
    }

    #[test]
    fn segment_handles_dialogue_quotes() {
        let text = "\"Wait for me!\" she called. He stopped.";
        let spans = segment_sentences(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "\"Wait for me!\" she called.");
        assert_eq!(spans[1].text, "He stopped.");
    }
}
