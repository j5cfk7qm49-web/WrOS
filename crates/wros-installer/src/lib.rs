//! Native presentation for the existing cdebconf passthrough protocol.
//! No partitioning, installation commands, privilege escalation or password
//! verification belongs here. A separate supervisor must own backend lifetime.
use std::{
    collections::{BTreeMap, BTreeSet},
    io::BufRead,
};
use zeroize::{Zeroize, Zeroizing};

pub mod ui;

pub const MAX_LINE: usize = 65_536;
const MAX_QUESTIONS: usize = 2048;
const MAX_STATE: usize = 8 * 1024 * 1024;
const MAX_ANSWER: usize = 4096;

/// Fixed diagnostics only: protocol values and answers never enter errors.
pub type Result<T> = std::result::Result<T, &'static str>;

/// Bounded, zeroed wire storage, including incomplete frames and EOF.
pub fn read_line(reader: &mut impl BufRead) -> Result<Option<Zeroizing<String>>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_LINE));
    loop {
        let available = reader
            .fill_buf()
            .map_err(|_| "Installer channel read failed")?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err("Incomplete installer frame")
            };
        }
        let count = available
            .iter()
            .position(|c| *c == b'\n')
            .map_or(available.len(), |i| i + 1);
        if bytes.len() + count > MAX_LINE {
            return Err("Installer frame too large");
        }
        let done = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if done {
            bytes.pop();
            let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid installer text")?;
            if text.contains(['\0', '\r', '\n']) {
                return Err("Invalid installer frame");
            }
            return Ok(Some(Zeroizing::new(text.to_owned())));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Password,
    Boolean,
    Select,
    Multiselect,
    Note,
    Error,
    Text,
}
impl Kind {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "string" => Self::String,
            "password" => Self::Password,
            "boolean" => Self::Boolean,
            "select" => Self::Select,
            "multiselect" => Self::Multiselect,
            "note" => Self::Note,
            "error" => Self::Error,
            "text" => Self::Text,
            _ => return Err("Unsupported installer question type"),
        })
    }
}

pub struct Question {
    pub kind: Kind,
    pub description: String,
    pub detail: String,
    pub choices: Vec<String>,
    values: Vec<String>,
    answer: Zeroizing<String>,
    pub selected: usize,
    pub checked: Vec<bool>,
}
impl Default for Question {
    fn default() -> Self {
        Self {
            kind: Kind::Text,
            description: String::new(),
            detail: String::new(),
            choices: vec![],
            values: vec![],
            answer: Zeroizing::new(String::with_capacity(MAX_ANSWER)),
            selected: 0,
            checked: vec![],
        }
    }
}
impl Question {
    fn prepare(&mut self) -> Result<()> {
        self.selected = 0;
        if matches!(self.kind, Kind::Select | Kind::Multiselect) {
            if self.choices.is_empty()
                || self.choices.len() != self.values.len()
                || self.choices.len() > 2048
                || self
                    .values
                    .iter()
                    .any(|value| value.len() > MAX_ANSWER || value.chars().any(char::is_control))
            {
                return Err("Invalid installer choices");
            }
            self.selected = self
                .values
                .iter()
                .position(|c| c == &*self.answer)
                .unwrap_or(0);
            let chosen = split_choices(&self.answer);
            self.checked = self.values.iter().map(|v| chosen.contains(v)).collect();
        } else if self.kind == Kind::Boolean {
            // Explicit consent is required again, including backend defaults
            // for partition-table, format and bootloader confirmations.
            self.answer.zeroize();
            self.answer.push_str("false");
        }
        Ok(())
    }
    pub fn shown_answer(&self) -> &str {
        if self.kind == Kind::Password {
            "[hidden]"
        } else {
            &self.answer
        }
    }
    pub fn type_char(&mut self, c: char) {
        if matches!(self.kind, Kind::String | Kind::Password)
            && !c.is_control()
            && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            && self.answer.len() + c.len_utf8() <= MAX_ANSWER
        {
            self.answer.push(c);
        }
    }
    pub fn erase(&mut self) {
        if matches!(self.kind, Kind::String | Kind::Password) {
            // String::pop leaves removed bytes in capacity. Zero the removed
            // character before shortening the answer.
            if let Some((at, _)) = self.answer.char_indices().last() {
                let prefix = Zeroizing::new(self.answer[..at].to_owned());
                self.answer.zeroize();
                self.answer.push_str(&prefix);
            }
        }
    }
    pub fn move_by(&mut self, amount: isize) {
        let count = if self.kind == Kind::Boolean {
            2
        } else {
            self.choices.len()
        };
        self.selected = self
            .selected
            .saturating_add_signed(amount)
            .min(count.saturating_sub(1));
    }
    pub fn toggle(&mut self) {
        if self.kind == Kind::Multiselect
            && let Some(checked) = self.checked.get_mut(self.selected)
        {
            *checked = !*checked;
        }
    }
    fn commit(&mut self) {
        let value = match self.kind {
            Kind::Boolean => Some(if self.selected == 1 {
                "true".into()
            } else {
                "false".into()
            }),
            Kind::Select => self.values.get(self.selected).cloned(),
            Kind::Multiselect => Some(
                self.values
                    .iter()
                    .zip(&self.checked)
                    .filter(|(_, checked)| **checked)
                    .map(|(v, _)| escape_choice(v))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            _ => None,
        };
        if let Some(value) = value {
            self.answer.zeroize();
            self.answer.push_str(&value);
        }
    }
}

pub struct Progress {
    pub min: i64,
    pub max: i64,
    pub value: i64,
    pub title: String,
    pub detail: String,
}
pub enum Exchange {
    Reply(Zeroizing<String>),
    Show,
}
fn reply(text: &str) -> Exchange {
    Exchange::Reply(Zeroizing::new(text.to_owned()))
}

pub struct Session {
    pub title: String,
    questions: BTreeMap<String, Question>,
    queued: Vec<String>,
    pub current: usize,
    pub showing: bool,
    pub allow_back: bool,
    pub progress: Option<Progress>,
    approved: BTreeSet<String>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            title: "Install WrOS".into(),
            questions: BTreeMap::new(),
            queued: vec![],
            current: 0,
            showing: false,
            allow_back: false,
            progress: None,
            approved: BTreeSet::new(),
        }
    }
}
impl Session {
    fn question(&mut self, tag: &str) -> Result<&mut Question> {
        if tag.is_empty() || tag.len() > 256 || !tag.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("Invalid installer question name");
        }
        if !self.questions.contains_key(tag) && self.questions.len() >= MAX_QUESTIONS {
            return Err("Too many installer questions");
        }
        Ok(self.questions.entry(tag.to_owned()).or_default())
    }
    pub fn active(&self) -> Option<&Question> {
        self.showing
            .then(|| self.queued.get(self.current))
            .flatten()
            .and_then(|tag| self.questions.get(tag))
    }
    pub fn active_mut(&mut self) -> Option<&mut Question> {
        if !self.showing {
            return None;
        }
        self.questions.get_mut(self.queued.get(self.current)?)
    }
    pub fn position(&self) -> (usize, usize) {
        (self.current + 1, self.queued.len())
    }
    pub fn advance(&mut self) -> Option<Zeroizing<String>> {
        self.active_mut()?.commit();
        if self.current + 1 < self.queued.len() {
            self.current += 1;
            None
        } else {
            self.showing = false;
            self.approved.extend(self.queued.drain(..));
            Some(Zeroizing::new("0".into()))
        }
    }
    pub fn back(&mut self) -> Option<Zeroizing<String>> {
        if !self.showing {
            return None;
        }
        if self.current > 0 {
            self.current -= 1;
            return None;
        }
        if !self.allow_back {
            return None;
        }
        self.showing = false;
        self.queued.clear();
        for q in self.questions.values_mut() {
            if q.kind == Kind::Password {
                q.answer.zeroize();
            }
        }
        Some(Zeroizing::new("30".into()))
    }
    /// The caller must stop the backend on Err or transport loss. cdebconf's
    /// passthrough module alone does not guarantee abort on an absent frontend.
    pub fn command(&mut self, line: &str) -> Result<Exchange> {
        if self.showing {
            return Err("Installer sent a command before acknowledgment");
        }
        let size: usize = self
            .questions
            .iter()
            .map(|(tag, q)| {
                tag.capacity()
                    + std::mem::size_of::<Question>()
                    + q.description.capacity()
                    + q.detail.capacity()
                    + (q.choices.capacity() + q.values.capacity()) * std::mem::size_of::<String>()
                    + q.choices.iter().map(String::capacity).sum::<usize>()
                    + q.values.iter().map(String::capacity).sum::<usize>()
                    + q.answer.capacity()
            })
            .sum();
        if size > MAX_STATE || line.len() > MAX_LINE || line.contains(['\n', '\r', '\0']) {
            return Err("Installer state too large or invalid");
        }
        let (command, args) = line.split_once(' ').unwrap_or((line, ""));
        match command {
            "CAPB" => {
                self.allow_back = args.split_whitespace().any(|s| s == "backup");
                Ok(reply("0 backup"))
            }
            "TITLE" => {
                self.title = display(&unescape(args));
                Ok(reply("0"))
            }
            "DATA" => {
                let (tag, rest) = args.split_once(' ').ok_or("Invalid installer data")?;
                let (field, value) = rest.split_once(' ').ok_or("Invalid installer data")?;
                let q = self.question(tag)?;
                match field {
                    "type" => q.kind = Kind::parse(value)?,
                    "description" => q.description = display(&unescape(value)),
                    "extended_description" => q.detail = display(&unescape(value)),
                    "choices" => {
                        q.choices = split_choices(&unescape(value))
                            .iter()
                            .map(|s| display(s))
                            .collect()
                    }
                    "choices-c" => q.values = split_choices(&unescape(value)),
                    _ => return Err("Unsupported installer field"),
                }
                if q.choices.len() > 2048 || q.values.len() > 2048 {
                    return Err("Too many installer choices");
                }
                Ok(reply("0"))
            }
            "SET" => {
                let (tag, value) = args.split_once(' ').ok_or("Invalid installer answer")?;
                if value.len() > MAX_ANSWER {
                    return Err("Installer answer too large");
                }
                let q = self.question(tag)?;
                q.answer.zeroize();
                let decoded = Zeroizing::new(unescape(value));
                if decoded.chars().any(char::is_control) {
                    return Err("Invalid installer answer characters");
                }
                q.answer.push_str(&decoded);
                Ok(reply("0"))
            }
            // cdebconf has already substituted these in the DATA descriptions.
            "SUBST" => {
                let mut parts = args.splitn(3, ' ');
                let tag = parts.next().ok_or("Invalid installer substitution")?;
                if parts.next().is_none() || parts.next().is_none() {
                    return Err("Invalid installer substitution");
                }
                self.question(tag)?;
                Ok(reply("0"))
            }
            "INPUT" => {
                let (priority, tag) = args.split_once(' ').ok_or("Invalid installer input")?;
                if !matches!(priority, "low" | "medium" | "high" | "critical")
                    || self.queued.iter().any(|q| q == tag)
                {
                    return Err("Invalid installer input queue");
                }
                if !self.questions.contains_key(tag) {
                    return Err("Unknown installer question");
                }
                self.approved.remove(tag);
                self.queued.push(tag.into());
                Ok(reply("0"))
            }
            "GO" if args.is_empty() => {
                for tag in &self.queued {
                    self.questions
                        .get_mut(tag)
                        .ok_or("Unknown installer question")?
                        .prepare()?;
                }
                self.current = 0;
                self.showing = !self.queued.is_empty();
                if self.showing {
                    Ok(Exchange::Show)
                } else {
                    Ok(reply("0"))
                }
            }
            "GET" => {
                if !self.approved.contains(args) {
                    return Err("Installer answer not approved");
                }
                let q = self
                    .questions
                    .get_mut(args)
                    .ok_or("Unknown installer answer")?;
                if q.answer.len() + 3 > MAX_LINE {
                    return Err("Installer answer frame too large");
                }
                let mut response = Zeroizing::new(String::from("0 "));
                response.push_str(&q.answer);
                if q.kind == Kind::Password {
                    q.answer.zeroize();
                }
                Ok(Exchange::Reply(response))
            }
            "PROGRESS" => {
                self.progress_command(args)?;
                Ok(reply("0"))
            }
            _ => Err("Unsupported installer command"),
        }
    }
    fn progress_command(&mut self, args: &str) -> Result<()> {
        let words: Vec<_> = args.split_whitespace().collect();
        let number = |s: &str| s.parse::<i64>().map_err(|_| "Invalid installer progress");
        match words.as_slice() {
            ["START", min, max, tag] => {
                let (min, max) = (number(min)?, number(max)?);
                if min < 0 || max <= min || max > i32::MAX as i64 {
                    return Err("Invalid installer progress range");
                }
                let q = self
                    .questions
                    .get(*tag)
                    .ok_or("Unknown installer progress title")?;
                self.progress = Some(Progress {
                    min,
                    max,
                    value: min,
                    title: q.description.clone(),
                    detail: q.detail.clone(),
                });
            }
            ["SET", value] | ["STEP", value] => {
                let progress = self
                    .progress
                    .as_mut()
                    .ok_or("Installer progress not started")?;
                let value = number(value)?;
                let value = if words[0] == "STEP" {
                    progress.value.saturating_add(value)
                } else {
                    value
                };
                if !(progress.min..=progress.max).contains(&value) {
                    return Err("Installer progress outside range");
                }
                progress.value = value;
            }
            ["INFO", tag] => {
                let q = self
                    .questions
                    .get(*tag)
                    .ok_or("Unknown installer progress information")?;
                let progress = self
                    .progress
                    .as_mut()
                    .ok_or("Installer progress not started")?;
                progress.detail = q.description.clone();
            }
            ["STOP"] => self.progress = None,
            _ => return Err("Invalid installer progress command"),
        }
        Ok(())
    }
}

pub fn display(s: &str) -> String {
    s.chars()
        .take(MAX_LINE)
        .map(|c| {
            if (c.is_control() && c != '\n')
                || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
            {
                ' '
            } else {
                c
            }
        })
        .collect()
}
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}
fn split_choices(s: &str) -> Vec<String> {
    let mut values = vec![];
    let mut value = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(c @ (',' | '\\')) => value.push(c),
                Some(c) => {
                    value.push('\\');
                    value.push(c);
                }
                None => value.push('\\'),
            },
            ',' => {
                values.push(value.trim().to_owned());
                value.clear();
            }
            _ => value.push(c),
        }
    }
    if !value.is_empty() || !values.is_empty() {
        values.push(value.trim().to_owned());
    }
    values
}
fn escape_choice(s: &str) -> String {
    s.replace('\\', "\\\\").replace(',', "\\,")
}

#[cfg(test)]
mod tests;
