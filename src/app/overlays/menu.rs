//! What each menu row does, in a line, and the help topic that says more:
//! the status line shows it while a menu is open, and `?` opens the topic.

use super::*;

/// What a menu row does, in a line, and the help topic that says more.
pub fn about_row(action: &Action, part: &str) -> (String, &'static str) {
    let (says, topic): (&str, &str) = match action {
        Action::Undo => ("Takes back the last change to the page.", "writing"),
        Action::Redo => ("Puts back the change Undo took away.", "writing"),
        Action::Cut => ("Moves the selected words to the clipboard.", "writing"),
        Action::Copy => ("Copies the selected words to the clipboard.", "writing"),
        Action::Paste => ("Puts the clipboard's words in at the caret.", "writing"),
        Action::SelectAll => ("Selects everything in the page.", "writing"),
        Action::Lock => (
            "Locks the writing session; your password opens it again. Everything is saved first.",
            "wros",
        ),
        Action::Sleep => (
            "Saves, locks and puts the computer to sleep, as Settings › System › Battery and power does.",
            "wros",
        ),
        Action::Restart => (
            "Saves everything, then restarts the computer after you confirm.",
            "wros",
        ),
        Action::ShutDown => (
            "Saves everything, then turns the computer off after you confirm.",
            "wros",
        ),
        Action::InstallToDisk => (
            "Installs WrOS on this computer's disk, encrypted. You confirm each step.",
            "wros",
        ),
        Action::BackUpNow => (
            "Backs this book up now, to the places Settings › Backups and accounts names.",
            "sync",
        ),
        Action::Side(crate::palette::Side::Revision) => (
            "Notes and TKs, footnotes, names, continuity, milestones and the revision dashboard.",
            "writing",
        ),
        Action::Side(crate::palette::Side::Notes) => (
            "The Pinboard, your journal, the scratchpad and the session handoff.",
            "writing",
        ),
        Action::Side(crate::palette::Side::Practice) => (
            "Story Spark, daily challenges, progress, goals, rest days, sprints and milestones.",
            "productivity",
        ),
        Action::SettingsGroup(group) => match group {
            crate::app::shell::Category::Writing => (
                "The editor, the manuscript, goals and revision settings.",
                "settings",
            ),
            crate::app::shell::Category::Interface => (
                "Themes, the look of the page, the keyboard and music.",
                "settings",
            ),
            crate::app::shell::Category::System => (
                "Wi-Fi, sound, the display, date and time, battery and power, and storage.",
                "wros",
            ),
            crate::app::shell::Category::Accounts => (
                "Backups, Google Drive, local accounts and your password.",
                "sync",
            ),
            crate::app::shell::Category::Updates => (
                "Updates to WrOS, going back a version, and recovery.",
                "wros",
            ),
        },
        Action::Settings => (
            "WrOS writing, interface, system and account settings; return to the same manuscript.",
            "wros",
        ),
        Action::Conflicts => (
            "A sync left two copies of a scene that disagree. Pick which words to keep.",
            "sync",
        ),
        Action::Export => (
            "Turn the book into a Word file, a PDF, an EPUB or a paperback, ready for readers.",
            "compile",
        ),
        Action::NewBook => (
            "A new novel, short story or blank book, or bring in a draft you already have. This one is saved first.",
            "books",
        ),
        Action::OpenBook => (
            "Switch to another book you've written in, or open one from its folder.",
            "books",
        ),
        Action::Quit => (
            "Everything is saved first. Next time, this book opens right where you left it.",
            "getting-started",
        ),
        Action::Corkboard => (
            "Every scene as an index card, to see the book at a glance and jot a synopsis, whose eyes, and how far along.",
            "corkboard",
        ),
        Action::NewScene => (
            "A new scene at the end of the chapter you're in. You name it first.",
            "outline",
        ),
        Action::NewChapter => (
            "A new chapter at the end of this part, already named for you.",
            "outline",
        ),
        Action::NewPart => ("", "outline"),
        Action::NewFolder => (
            "A folder beside the one selected, for anything that isn't a chapter.",
            "outline",
        ),
        Action::Rename => (
            "A new name for the scene or folder you're on. Its file is renamed to match.",
            "outline",
        ),
        Action::Delete => (
            "Moves it to the Trash at the bottom of the outline. It asks first, by name.",
            "outline",
        ),
        Action::Goals => (
            "How many words the book is aiming for, and each day. Try a month of drafting: 50,000 words.",
            "sprints",
        ),
        Action::NextTk => (
            "Jumps to the next TK or note after the cursor, going round to the top at the end.",
            "notes",
        ),
        Action::OpenCodex => (
            "With the cursor on a name from your notebook, opens that character's or place's note beside the page.",
            "notebook",
        ),
        Action::Timer => (
            "The Pomodoro: twenty-five minutes of writing, then five of rest. Again pauses it.",
            "pomodoro",
        ),
        Action::TimerReset => ("Sets the Pomodoro back to the start.", "pomodoro"),
        Action::SaveSession => (
            "Writes this sitting into your writing sessions now, instead of when you quit.",
            "sessions",
        ),
        Action::Compile => (
            "The whole manuscript as one Markdown file, in the book's exports folder.",
            "compile",
        ),
        Action::ProjectMap => (
            "Rewrites project.md, a one-page map of the book: every part, chapter and scene, with word counts.",
            "compile",
        ),
        Action::Details => (
            "This scene's status, POV, synopsis and word target, and whether it goes in exports. i in the outline.",
            "outline",
        ),
        Action::Restore => (
            "Takes what's highlighted in the Trash back to where it was, in its old folder under its old name.",
            "outline",
        ),
        Action::Italic => (
            "Italics on the selected words, or the word at the cursor. Again takes them off. They're *stars* in the file.",
            "writing",
        ),
        Action::Bold => (
            "Bold on the selected words, or the word at the cursor. Again takes it off. It's **double stars** in the file.",
            "writing",
        ),
        Action::MoveUp => (
            "Moves the scene or folder you're on up one place. At the top of a chapter, a scene crosses into the one before.",
            "outline",
        ),
        Action::MoveDown => (
            "Moves the scene or folder you're on down one place. At the bottom of a chapter, a scene crosses into the next.",
            "outline",
        ),
        Action::History => (
            "Every saved version of this scene, to compare with now or bring back. Nothing is ever lost.",
            "history",
        ),
        Action::FindInBook => (
            "Search every scene and note for a word or phrase, and replace it everywhere if you like.",
            "find",
        ),
        Action::Sessions => (
            "Your writing sittings, like a diary: when you wrote, where, and how many words. Each one can be looked back at.",
            "sessions",
        ),
        Action::FindInScene => (
            "Find a word in this scene, and replace it if you like. Press it again to search the whole book.",
            "find",
        ),
        Action::CheckNames => (
            "Finds names spelled two ways, like Oren and Orin, so a character stays the same person all the way through.",
            "notebook",
        ),
        Action::MoveHistoryOut => (
            "Moves this book's writing history out of the synced folder, where sync can't damage it.",
            "sessions",
        ),
        Action::FocusMode => (
            "Hides everything but the page, so it's just you and the words. Esc still opens this menu.",
            "focus",
        ),
        Action::BesidePicker => (
            "Shows another scene next to this one, to read from while you write.",
            "beside",
        ),
        Action::NotesList => (
            "Every note and TK (your mark for \"fill this in later\") in the book, in one list.",
            "notes",
        ),
        Action::InsertFootnote => (
            "Create a unique Markdown footnote reference at the cursor and begin its definition. Undo restores the scene.",
            "notes",
        ),
        Action::FootnoteAtCaret => (
            "Read the public footnote at the caret and navigate its references or original definition.",
            "notes",
        ),
        Action::Footnotes => (
            "Browse Markdown footnote references and definitions, inspect missing labels, and jump to the original scene.",
            "writing",
        ),
        Action::NextDraft => (
            "Opens the next scene that isn't marked revised or done yet. Handy for a revision pass.",
            "revision",
        ),
        Action::EchoWords => (
            "Lights up a word you've used again too soon, like \"the lamp… the lamp\" a line apart. Easy to miss, easy to fix.",
            "revision",
        ),
        Action::StartSprint => (
            "A timed burst of writing with a word goal. The status bar keeps count.",
            "sprints",
        ),
        Action::EndSprint => ("Ends the sprint now.", "sprints"),
        Action::ReadAloud => (
            "Reads the scene out loud from where you are, highlighting each sentence. Hearing it catches what the eye skips.",
            "revision",
        ),
        Action::Voices => (
            "Which voice reads aloud: a natural Piper voice if you have one, or your computer's own.",
            "revision",
        ),
        Action::Progress => (
            "Your words each week, your pace lately, and roughly when you'll reach your goal. No streaks.",
            "sprints",
        ),
        Action::MusicPlayer => (
            "The queue, your playlists, search, and every control.",
            "music",
        ),
        Action::Themes => (
            "Pick your colours. Each one shows as you move; Esc puts the old one back.",
            "themes",
        ),
        Action::MusicSource => (
            "Where music comes from: YouTube Music, Spotify, Jellyfin or Plex.",
            "music",
        ),
        Action::MusicToggle => (
            "Music is off until you want it. On, it gets a pane of its own.",
            "music",
        ),
        Action::Spellcheck => (
            "Underlines words that might be misspelt as you write.",
            "spelling",
        ),
        Action::SpellingLanguage => (
            "Which English spellcheck knows: American, British, Canadian or Australian. Each press tries the next.",
            "spelling",
        ),
        Action::Icons => (
            "A little symbol beside each row of the outline.",
            "settings",
        ),
        Action::LineWidth => (
            "How long a line of prose can run. Each press tries the next width.",
            "settings",
        ),
        Action::Typewriter => (
            "In focus mode, keeps the line you're writing near the middle of the screen.",
            "focus",
        ),
        Action::ManuscriptLook => (
            "How the exported manuscript looks: the font, paper, spacing and chapter headings.",
            "compile",
        ),
        Action::AuthorDetails => (
            "Your name and contact details, for the manuscript's title page.",
            "compile",
        ),
        Action::Help => (
            "Everything WrOS does, in plain words. Type to search it.",
            "getting-started",
        ),
        Action::About => (
            "Which version this is, and who made it: Catfinity Studios, makers of Catfinity.",
            "getting-started",
        ),
        Action::License => (
            "Free to use, even for work you sell. Credit is all it asks.",
            "license",
        ),
        Action::Donate => (
            "Grimoire is free. A tip on Ko-fi helps it keep growing, and is never required.",
            "getting-started",
        ),
        Action::Dashboard => (
            "Your book at a glance: words, goals, sprints and what is due today.",
            "wros",
        ),
        Action::SystemSettings => ("Shows local system status and safe OS controls.", "wros"),
        Action::Workspaces => ("Restores a saved writing layout.", "wros"),
        Action::Tutorial => ("Opens replayable, event-verified lessons.", "wros"),
        Action::ToggleOutline => ("Shows or hides the outline independently.", "wros"),
        Action::Scratchpad => (
            "Original Notes linked to scenes; explicit independent promotion keeps the original.",
            "scratchpad",
        ),
        Action::Handoff => (
            "Original next-session prose and explicit saved manuscript context resume.",
            "handoff",
        ),
        Action::Continuity => (
            "Track explicit story facts, ordered timeline events and plot threads without hallucinating conflicts.",
            "continuity",
        ),
        Action::Milestones => (
            "Save named whole-book draft snapshots and reversible alternative scene versions.",
            "continuity",
        ),
        Action::ReadChapterAloud => (
            "Queue the open scene and following chapter scenes for continuous offline read-aloud.",
            "continuity",
        ),
        Action::ReadManuscriptAloud => (
            "Queue the open scene and all following manuscript scenes across chapters for offline read-aloud.",
            "revision",
        ),
        Action::ReviewNotes => (
            "Passage-anchored local comments, guarded revision review and explicit re-anchoring",
            "review-notes",
        ),
        Action::ProseDashboard => (
            "Local advisory prose statistics, pinned offline tools, source-safe ranked review and selected-reference voice profiles",
            "prose-dashboard",
        ),
        Action::Lexicon => (
            "Read-only offline WordNet senses, definitions, synonyms and explicit sense-specific antonyms; no generated definitions or automatic manuscript edits.",
            "lexicon",
        ),
        Action::WritingChecks => (
            "Opt-in local Harper grammar, original-source highlights and explicit undoable review; no automatic prose edits or mandatory model use.",
            "writing-checks",
        ),
        Action::StorySpark => (
            "Offline original plots/exercises, compatible combinations, saved civil-date daily results, reference-only history and explicit source-safe actions.",
            "story-spark",
        ),
        Action::Productivity
        | Action::MeasuredGoals
        | Action::Calendar
        | Action::AdvancedSprints
        | Action::Achievements
        | Action::DailyChallenges => (
            "Real operation ledger, explicit goals/rest days, monotonic sprints, optional milestones and persisted offline challenges; not baseline/Git-derived gross or time.",
            "productivity",
        ),
        Action::Journal => (
            "Recorded saved-manuscript observations, separate from original Markdown reflections. No gross-word or active-time guesses.",
            "journal",
        ),
        Action::Pinboard
        | Action::PinCurrent
        | Action::TogglePins
        | Action::PinDocument(_)
        | Action::RelinkPin(_, _) => (
            "Persistent project Markdown references, independent card scroll, explicit guarded source editing; no copied bodies.",
            "pins",
        ),
        Action::ToggleSecondary => (
            "Shows or hides an independent editable document; its undo and selection stay intact.",
            "wros",
        ),
        Action::SecondaryPicker | Action::EditSecondary(_) => (
            "Chooses a scene, note or research document for the independent editor. One buffer owns each file.",
            "wros",
        ),
        _ => ("", "getting-started"),
    };
    let says = if *action == Action::NewPart {
        format!("A new {part} at the end of the manuscript: the biggest division of the book.")
    } else {
        says.to_string()
    };
    (says, topic)
}
