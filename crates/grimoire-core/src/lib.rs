//! Grimoire's heart: the book on disk, and everything that reads or changes it.
//!
//! No terminal, no windows, no audio — nothing here knows how it is being
//! shown. That is what lets the same code back the terminal app on a desktop
//! and an Android app on a phone, with the book itself staying plain Markdown
//! either way.

pub mod accounting;
pub mod atomic;
pub mod challenges;
pub mod cloud;
pub mod codex;
pub mod cork;
pub mod create;
pub mod days;
pub mod diagnostics;
pub mod documents;
pub mod editor;
pub mod export;
pub mod export_print;
pub mod history;
pub mod import;
pub mod journal;
pub mod lexicon;
pub mod manuscript;
pub mod measure;
pub mod names;
pub mod notes;
pub mod paths;
pub mod pdf;
pub mod pins;
pub mod productivity;
pub mod project;
pub mod prose;
pub mod prose_tools;
pub mod recovery;
pub mod resume;
pub mod revision;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod spark;
pub mod spell;
pub mod statefile;
pub mod submission;
pub mod sync;
pub mod typeset;

pub mod tutorial;
pub mod workflow;
pub mod workspaces;

pub mod voice_profile;

pub mod review_queue;

pub mod annotations;
pub mod continuity;
pub mod footnotes;
pub mod milestones;
pub mod preflight;
pub mod speech;
mod story_state;
