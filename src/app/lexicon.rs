//! Offline, read-only WordNet reader. The only authoring action is explicit
//! copy into the existing clipboard; native selection/paste/undo stays owned
//! by the editor. One active lookup plus one coalesced newest request.
use super::*;
use grimoire_core::lexicon::{self as wordnet, Dictionary, Lookup, Sense};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::{Block, Clear, Paragraph},
};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{self, Receiver, TryRecvError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Query,
    Results,
    Sense,
}
type Reply = (u64, std::result::Result<Lookup, String>);
struct Request {
    id: u64,
    query: String,
    root: PathBuf,
}
pub struct Lexicon {
    pub page: Page,
    pub query: String,
    pub result: Option<Lookup>,
    pub selected: usize,
    pub word: usize,
    pub antonyms: bool,
    pub notice: String,
    pub data_root: PathBuf,
    pub launched: u64,
    pub discarded: u64,
    generation: u64,
    receiver: Option<Receiver<Reply>>,
    queued: Option<Request>,
    scroll: Option<usize>,
    fit: Cell<(usize, usize)>,
}
impl Default for Lexicon {
    fn default() -> Self {
        Self {
            page: Page::Query,
            query: String::new(),
            result: None,
            selected: 0,
            word: 0,
            antonyms: false,
            notice: String::new(),
            data_root: std::env::var_os("WRITINGDECK_WORDNET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(wordnet::DEFAULT_ROOT)),
            launched: 0,
            discarded: 0,
            generation: 0,
            receiver: None,
            queued: None,
            scroll: None,
            fit: Cell::new((0, 0)),
        }
    }
}
impl Lexicon {
    fn cancel(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.queued = None;
        self.result = None;
        // Existing read-only I/O can finish; no thread join blocks input.
    }
    fn start(&mut self, request: Request) {
        let (tx, rx) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("wordnet-lookup".into())
            .spawn(move || {
                let id = request.id;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    Dictionary::open(&request.root)?.lookup(&request.query)
                }))
                .map_err(|_| "Local WordNet reader failed; manuscript unchanged.".to_owned())
                .and_then(|r: anyhow::Result<Lookup>| r.map_err(|e| format!("{e:#}")));
                let _ = tx.send((id, result));
            }) {
            Ok(_) => {
                self.launched = self.launched.saturating_add(1);
                self.receiver = Some(rx);
            }
            Err(e) => {
                self.notice = format!(
                    "Dictionary worker unavailable: {e}. Enter explicitly retries; no source changed."
                )
            }
        }
    }
    fn request(&mut self) {
        if let Err(e) = wordnet::normalize_query(&self.query) {
            self.notice = e.to_string();
            return;
        }
        let Some(next) = self.generation.checked_add(1) else {
            self.notice =
                "Lookup generation exhausted; reopen application. No source changed.".into();
            return;
        };
        self.generation = next;
        self.result = None;
        self.selected = 0;
        self.word = 0;
        self.scroll = None;
        self.page = Page::Results;
        self.notice = "Local dictionary lookup pending; writing remains independent. No network/model/download.".into();
        let request = Request {
            id: next,
            query: self.query.clone(),
            root: self.data_root.clone(),
        };
        if self.receiver.is_some() {
            self.queued = Some(request);
        } else {
            self.start(request);
        }
    }
    fn tick(&mut self) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok((id, result)) => {
                self.receiver = None;
                if id == self.generation {
                    match result {
                        Ok(result) => {
                            self.notice = if result.senses.is_empty() {
                                "No indexed senses found for this query/candidates. This is not a spelling or literary judgment.".into()
                            } else {
                                "Choose a specific sense. Synonyms/opposites are not interchangeable in every context.".into()
                            };
                            self.result = Some(result);
                        }
                        Err(e) => {
                            self.notice = format!(
                                "WordNet unavailable/invalid: {e}. No definition invented, no data downloaded or source changed."
                            )
                        }
                    }
                } else {
                    self.discarded = self.discarded.saturating_add(1);
                }
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                self.notice =
                    "Dictionary worker disconnected; Enter explicitly retries. Source unchanged."
                        .into();
            }
        }
        if let Some(request) = self.queued.take() {
            self.start(request);
        }
    }
    fn sense(&self) -> Option<&Sense> {
        self.result.as_ref()?.senses.get(self.selected)
    }
    fn words(&self) -> Vec<String> {
        self.sense().map_or_else(Vec::new, |s| {
            if self.antonyms {
                s.antonyms.iter().map(|a| a.word.clone()).collect()
            } else {
                s.words.clone()
            }
        })
    }
}
impl App {
    pub fn open_lexicon(&mut self) {
        let selected = self
            .editor
            .selected_text()
            .filter(|s| s.chars().count() <= 96);
        let word = selected
            .or_else(|| {
                let (line, a, b) = self.editor.word_at_cursor()?;
                Some(
                    self.editor.lines[line]
                        .chars()
                        .skip(a)
                        .take(b - a)
                        .collect(),
                )
            })
            .filter(|s: &String| wordnet::normalize_query(s).is_ok())
            .unwrap_or_default();
        self.lexicon.cancel();
        self.lexicon.query = word;
        self.lexicon.page = Page::Query;
        self.lexicon.scroll = None;
        self.lexicon.notice = "Read-only local WordNet 3.0. Enter a word/phrase; Enter looks up. Opening never edits/selects the manuscript.".into();
        self.overlay = Overlay::Lexicon;
    }
    pub fn tick_lexicon(&mut self) {
        self.lexicon.tick();
    }
    pub fn paste_lexicon(&mut self, text: &str) {
        if self.lexicon.page != Page::Query {
            self.lexicon.notice = "Reader: pasted commands ignored; no source changed.".into();
            return;
        }
        let remaining = 96usize.saturating_sub(self.lexicon.query.chars().count());
        self.lexicon.query.extend(
            text.chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .take(remaining),
        );
        self.lexicon.notice =
            "Pasted query text only; Enter explicitly looks up. No manuscript edit.".into();
    }
    pub fn on_lexicon_key(&mut self, key: Key) {
        self.lexicon.scroll = match key {
            Key::PageDown => {
                let (at, max) = self.lexicon.fit.get();
                Some(at.saturating_add(10).min(max))
            }
            Key::PageUp => {
                let (at, _) = self.lexicon.fit.get();
                Some(at.saturating_sub(10))
            }
            _ => None,
        };
        match self.lexicon.page {
            Page::Query => match key {
                Key::Esc => {
                    self.lexicon.cancel();
                    self.overlay = Overlay::None;
                }
                Key::Enter => self.lexicon.request(),
                Key::Backspace => {
                    self.lexicon.query.pop();
                }
                Key::Delete => self.lexicon.query.clear(),
                Key::Char(c) if !c.is_control() && self.lexicon.query.chars().count() < 96 => {
                    self.lexicon.query.push(c)
                }
                _ => {}
            },
            Page::Results => match key {
                Key::Esc | Key::Char('e') => {
                    self.lexicon.cancel();
                    self.lexicon.page = Page::Query;
                }
                Key::Enter => {
                    if self.lexicon.sense().is_some() {
                        self.lexicon.page = Page::Sense;
                        self.lexicon.word = 0;
                        self.lexicon.antonyms = false;
                        self.observe_deck(grimoire_core::tutorial::Event::DictionarySenseReviewed);
                    }
                }
                Key::Char('r') => self.lexicon.request(),
                Key::Up => self.lexicon.selected = self.lexicon.selected.saturating_sub(1),
                Key::Down => {
                    let len = self.lexicon.result.as_ref().map_or(0, |r| r.senses.len());
                    self.lexicon.selected = self
                        .lexicon
                        .selected
                        .saturating_add(1)
                        .min(len.saturating_sub(1));
                }
                Key::Home => self.lexicon.selected = 0,
                Key::End => {
                    self.lexicon.selected = self
                        .lexicon
                        .result
                        .as_ref()
                        .map_or(0, |r| r.senses.len().saturating_sub(1))
                }
                _ => {}
            },
            Page::Sense => {
                match key {
                    Key::Esc => self.lexicon.page = Page::Results,
                    Key::Tab | Key::Left | Key::Right => {
                        self.lexicon.antonyms = !self.lexicon.antonyms;
                        self.lexicon.word = 0;
                    }
                    Key::Up => self.lexicon.word = self.lexicon.word.saturating_sub(1),
                    Key::Down => {
                        self.lexicon.word = self
                            .lexicon
                            .word
                            .saturating_add(1)
                            .min(self.lexicon.words().len().saturating_sub(1))
                    }
                    Key::Home => self.lexicon.word = 0,
                    Key::End => self.lexicon.word = self.lexicon.words().len().saturating_sub(1),
                    Key::Char('c') => {
                        if let Some(word) = self
                            .lexicon
                            .words()
                            .get(self.lexicon.word)
                            .filter(|s| s.len() <= 1024)
                        {
                            self.clip = word.clone();
                            let system = self.background && copy_to_clipboard(word);
                            self.lexicon.notice = format!(
                                "Copied {word:?} {}. No source change; Esc through senses/query returns to the original editor. Native selection/paste/undo remains yours.",
                                if system {
                                    "to clipboard"
                                } else {
                                    "for Ctrl-V here"
                                }
                            );
                        } else {
                            self.lexicon.notice = "No explicit word/pointer in this sense to copy. No opposite inferred.".into();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
fn plain(s: &str, limit: usize) -> String {
    s.chars()
        .take(limit)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
pub fn draw(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Dictionary and thesaurus · local WordNet 3.0 ")
        .border_style(Style::default().fg(theme.moon));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(4),
    ])
    .areas(inner);
    f.render_widget(
        Paragraph::new(format!(
            "{} · no model / network / automatic replacement\n{}",
            wordnet::VERSION,
            plain(&app.lexicon.notice, 2048)
        ))
        .style(Style::default().fg(theme.dim)),
        header,
    );
    let mut rows = vec![];
    let mut selected = None;
    match app.lexicon.page {
        Page::Query => {
            rows.push(format!("Search word or phrase: {}▏", app.lexicon.query));
            rows.push("Enter looks up all parts of speech and explicit inflection candidates. Delete clears the field.".into());
            rows.push("Synonyms depend on the chosen sense. Antonyms follow explicit word/synset pointers, not guessed opposites.".into());
            rows.push("No query/result is saved as project prose or history. Missing/unsupported data is reported, never downloaded at runtime.".into());
            rows.push("Resource: Princeton WordNet 3.0; permissive notice retained with wordnet-base. Historic definitions are not endorsed or style rules.".into());
        }
        Page::Results => {
            if let Some(result) = &app.lexicon.result {
                rows.push(format!(
                    "{:?}: {} indexed senses{}",
                    result.query,
                    result.senses.len(),
                    if result.truncated {
                        " (64-sense cap; not complete)"
                    } else {
                        ""
                    }
                ));
                for (i, sense) in result.senses.iter().enumerate() {
                    if i == app.lexicon.selected {
                        selected = Some(rows.len());
                    }
                    rows.push(format!(
                        "{} {} · {} #{}{} · {}",
                        if i == app.lexicon.selected {
                            "›"
                        } else {
                            " "
                        },
                        sense.lemma,
                        sense.id.part.name(),
                        sense.number,
                        if sense.morphological {
                            " (index-verified morphology)"
                        } else {
                            ""
                        },
                        plain(&sense.gloss, 130)
                    ));
                }
            } else {
                rows.push(app.lexicon.notice.clone());
                rows.push(
                    "No current result. A pending lookup is not a clean/empty dictionary result."
                        .into(),
                );
            }
        }
        Page::Sense => {
            if let Some(sense) = app.lexicon.sense() {
                rows.push(format!(
                    "{} · {} sense {} · synset byte {}{}",
                    sense.lemma,
                    sense.id.part.name(),
                    sense.number,
                    sense.id.offset,
                    if sense.adjective_satellite {
                        " · adjective satellite"
                    } else {
                        ""
                    }
                ));
                if sense.morphological {
                    rows.push(format!("Query {:?} resolved to indexed lemma {:?}; manuscript spelling is unchanged.",app.lexicon.query,sense.lemma));
                }
                rows.push(sense.gloss.clone());
                rows.push(
                    if app.lexicon.antonyms {
                        "EXPLICIT ANTONYM POINTERS (Tab changes to synonyms)"
                    } else {
                        "SYNSET WORDS / SYNONYMS (Tab changes to explicit antonyms)"
                    }
                    .into(),
                );
                let words = app.lexicon.words();
                if words.is_empty() {
                    rows.push(
                        "No explicit antonym for this queried word/sense; none inferred.".into(),
                    );
                }
                for (i, word) in words.iter().enumerate() {
                    if i == app.lexicon.word {
                        selected = Some(rows.len());
                    }
                    rows.push(format!(
                        "{} {}",
                        if i == app.lexicon.word { "›" } else { " " },
                        word
                    ));
                    if app.lexicon.antonyms {
                        rows.push(format!("  {}", sense.antonyms[i].gloss));
                    }
                }
                rows.push("Copy is explicit. It does not replace a source word; return to the original editor and use ordinary selection/paste if desired.".into());
            }
        }
    }
    let width = body.width.saturating_sub(2).max(1) as usize;
    let mut wrapped = vec![];
    let mut selected_line = 0;
    for (index, row) in rows.iter().enumerate() {
        if Some(index) == selected {
            selected_line = wrapped.len();
        }
        wrapped.extend(crate::ui::wrap_words(&plain(row, MAX_DISPLAY_CHARS), width));
    }
    let height = body.height as usize;
    let max = wrapped.len().saturating_sub(height);
    let offset = app
        .lexicon
        .scroll
        .unwrap_or_else(|| selected_line.saturating_sub(height.saturating_sub(2)))
        .min(max);
    app.lexicon.fit.set((offset, max));
    let lines: Vec<_> = wrapped
        .into_iter()
        .skip(offset)
        .take(height)
        .map(Line::from)
        .collect();
    f.render_widget(
        Paragraph::new(lines).style(Style::default().fg(theme.text)),
        body,
    );
    let hint = match app.lexicon.page {
        Page::Query => "Type query · Enter lookup · Backspace edit · Delete clear · Esc original",
        Page::Results => "↑↓ choose sense · Enter definition · r retry · e/Esc edit query",
        Page::Sense => "↑↓ choose word · Tab synonyms/antonyms · c explicit copy · Esc senses",
    };
    f.render_widget(
        Paragraph::new(format!(
            "{hint}\nPgUp/PgDn scroll · F1 help · no automatic insertion or literary-quality score"
        ))
        .style(Style::default().fg(theme.dim))
        .wrap(ratatui::widgets::Wrap { trim: false }),
        footer,
    );
}
const MAX_DISPLAY_CHARS: usize = 16_384;
