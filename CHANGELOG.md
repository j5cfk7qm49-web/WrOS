# Changelog

## 0.1.0~rc33 · Firmware for your laptop

- WrOS picks the firmware your laptop needs when you install from a WrOS USB
  stick and checks again when you update. The stick carries the supported
  firmware for offline installs. The rc33 update includes the HP Stream 11's
  Wi-Fi, graphics and sound firmware for the move from rc32.
- **Go back** brings back the firmware saved with the previous version of WrOS.
- Plug in a new device and choose *Settings › Updates and recovery › Updates ›
  Firmware for a new device*. WrOS looks for matching firmware on the update
  site or on a WrOS USB stick when you're offline.

## 0.1.0~rc32 · Where your project is kept, a copy in one file, Grimoire 0.9

WrOS's writing app now includes Grimoire 0.8 and 0.9.

**Where your project is, and a copy as one file**

- *Project › This project › Where this project is kept…* shows the folder,
  how many files it has and their size, whether it's synced to Google Drive,
  and your last backup (or, without one, your last saved copy). `o` opens
  the folder, `s` saves a copy, and `m` moves a project from elsewhere into
  `Documents`.
- *Project › This project › Save a copy (one file)* makes a ZIP of the
  project, named for it and the day, in `Documents/Copies`. It never
  replaces an earlier copy. If an edit is still waiting to be saved, it asks
  you to wait a moment first.
- *Open a copy…* on the projects list opens a ZIP as a project of its own.
  The ZIP stays as it was, and nothing already there is touched.

**New in the editor and outline**

- A caret in every one-line box (names, synopsis, find and replace…):
  `←` `→` `Home` `End` and `Delete` work in them.
- Empty the Trash with `d` on the Trash itself, after one question. It
  deletes only what the question named. `u` brings one scene back out of a
  deleted chapter.
- In the whole-project search, `Enter` in the replace box changes one match
  and moves to the next. `Ctrl-R` still changes them all, after asking.
- Read aloud goes slower or faster with `-` and `+`, and remembers it.
- Template sheets: `f` on a sheet in the outline makes a new note from it.
- On the corkboard, `c` adds a chapter and asks for its first scene.
- Taught words: *Settings › Writing › Editor › Taught words* lists every word
  spellcheck was told to ignore; `d` takes one back.
- Menus take mouse clicks, and right-click in the outline opens the Project
  menu for that row.

**Your words, safer**

- A paste goes only into a text field, never runs as keys over a list.
- Renaming a note or a chapter keeps every `[[link]]` to it.
- Chapter 100 comes after chapter 99. A scene with Windows line endings
  keeps its front matter. Quotes in a synopsis come back as typed.
- Exports keep every chapter of a part with a loose scene in it; Interludes
  and Codas no longer break the EPUB; the paperback PDF has even margins.
- Drafts in older Windows encodings or UTF-16 come in as written, and
  hard-wrapped paragraphs are joined back up.
- Words on a highlighted row stay readable in every theme.

**Under the hood**

- WrOS's system is built on the newest NixOS 26.05 and Rust 1.100, with
  the writing app's libraries brought up to date.
- Firmware comes in one package per driver. The installed system carries
  only what the HP Stream 11 needs (its Wi-Fi, graphics and sound), so the
  system is smaller; the ISO still carries Wi-Fi firmware for other laptops.

## 0.1.0~rc31 · Square view tiles

- The W, r, O and S tiles at the left of the top bar, and the ⏻ tile at the
  right, are now squares two characters wide, with the letter clear of the
  frame. The current view's tile is filled in, and the S tile still turns red
  when cards are due. On the text console the letters are spaced out the same
  way (W r O S).

## 0.1.0~rc30 · New top and bottom bars, the views renamed, fixes from the laptop

- The views are now **Write, read, Outline and Study**, after the letters of
  the WrOS logo. Four tiles at the left of the top bar switch between them
  (or Super+1 to 4); the S tile turns red when cards are due. Your notes,
  reading and cards live in `Documents/Commonplace` (a `Documents/Brain`
  folder is moved there once).
- The WrOS menu is now the ⏻ tile at the right of the top bar: Lock, Sleep,
  Restart, Shut down (Esc, then W, still opens it). About WrOS is in Help.
- Menus drop down in the bar's colour with a shadow.
- Two bars under the page. The upper one says only what the screen doesn't
  already show (the project, the scene's status, a selection's word count),
  and its Wi-Fi, sound, battery and clock open small panels: networks,
  volume and the playing track, brightness and night light, the date and a
  month calendar. The lower one shows the project's progress and today's
  words, with messages at the right; the playing track scrolls there when
  nothing else is.
- Focus mode shows only the page. A line appears only if a save fails, a
  conflict waits or the battery is about to suspend. A soft chime marks the
  end of each Pomodoro phase (Settings › Writing › Editor).
- A wrong password at the login screen asks again, with the name kept,
  instead of restarting the login screen (#175).
- The brightness and volume keys respond faster: a small native program
  handles each press instead of starting Python (#174).
- Updates are smaller: releases are built from the exact committed files,
  so packages that didn't change are no longer sent again.

## 0.1.0~rc29 · Catalogue search and waiting for the previous project's sync

- Type `@` in an essay to search Open Library and Crossref as well as your
  project and Library: title, author, ISBN or DOI. Press → on a book to see
  its editions, or paste a web address to cite the page. The pick fills in
  the source and its Works Cited entry. **Add a source** searches the same
  way. Online search is on by default and can be turned off in
  Settings › Writing › Manuscript; project and Library sources stay
  available offline.
- Exporting an MLA paper that cites a source picked from the Library works
  again (rc27 and rc28 stopped with "Check the details for source …").
- Restart, Shut down and Quit wait for the previous project's Drive sync
  after a project switch, as they already did for the open project's.
  Quit says which project is still syncing. If that sync ends with a
  problem, WrOS says so once, and the project syncs again when you open it.

## 0.1.0~rc28 · Navigation, save and sync fixes

- Switch between Write, Library, Brain and Review from View. Commands for
  selected project items require Write, and opening a project search result
  returns to its source. Find anything keeps the highlighted result visible
  on short screens.
- Library shows the active filter and page controls. Catalog replies stay
  out of the way while another Library prompt is open. Brain, Library and
  Review text fields support keyboard undo and redo.

- Open another project as soon as local saves finish. The previous project's
  Drive sync continues in the background.
- Pause essay export until the current manuscript can be saved.
- Save a Brain note and its new Library backlinks together. Review keeps
  the history of a cloze whose prompt stays the same when a duplicate is removed.
- Use less work to count and select due cards. Reuse an existing attachment
  without writing a temporary copy of it.

## 0.1.0~rc27 · Library

- Browse and filter Library works by title, author, identifier or tag. A
  work can have separate editions. Identical attachments share one file.
- Import BibLaTeX and CSL-JSON, or look up a DOI or ISBN before adding an edition.
- The `@` picker links to a Library edition in Brain and can cite one in
  Write. Papers keep their accepted bibliography. Review edition changes
  on project open or export and choose whether to use them.
- The optional PDF and OCR benchmark measures rendering and recognition
  using PDFs chosen by the writer.

## 0.1.0~rc26 · Review cards

- Make a card in a Brain note with `prompt :: answer`, a two-way card with
  `prompt ::: answer`, or a blank with `{{hidden text}}`. For a list answer,
  leave the answer empty and add child notes.
- Open **Review cards** to reveal answers and grade recall from 0 to 5. Filter
  a session by tag or saved search, and see how many cards are due in the view bar.
- Card identities and grades live in the Brain journal and survive rebuilding
  the search cache. The schedule uses the pinned FSRS-6 parameters.

## 0.1.0~rc25 · Brain

- Brain opens to Today and supports named pages, nested notes, links, backlinks,
  tags, editable fields and saved search lists.
- Write can open the project's notes beside the manuscript. Brain and Write
  keep their own drafts and focus.
- Brain writes readable Markdown copies and can be backed up independently.
  Restoring a Brain snapshot verifies its contents in a separate folder.

## 0.1.0~rc16 · The installer, fixed on real hardware

The installer fixes found installing rc14 on the HP Stream, with a new USB stick image, and the
fix that lets an installed laptop take updates from Settings at all.

- **Updates from Settings.** rc14 and rc15's app refused the NixOS updater's replies (extra
  fields in review, stage, go-back and apply), so an update fetched online could not be
  reviewed or installed (#143). The app accepts and checks them now, and a contract test
  compares every updater reply with the app's parser. rc14 and rc15 laptops cannot take this
  fix from Settings: they reach rc16 with one install from a root shell.

- **The stick.** A stick written by copying the ISO's files (Rufus's default "ISO image mode")
  starts: the live system mounts its medium whatever filesystem carries the label (#132).
- **Unlocking the new disk.** The install no longer hangs for ten minutes and fails when it
  opens the encrypted disk: the installer's sandbox no longer has its own IPC namespace, which
  kept udev from answering cryptsetup (#136).
- **Copying WrOS.** nixos-install no longer refuses the target because the installer's working
  folder was private (#137). The copy may take up to an hour, for slow sticks and storage.
- **Trying again.** A failed install no longer blocks every later attempt until a restart (#138).
- **Questions.** The hostname, username and passwords are checked as you answer, with what is
  allowed, instead of failing after you confirm the erase (#139).
- **Keyboard.** Choosing a layout switches the installer's keyboard at once, so the passwords
  you type next are the ones the unlock screen expects (#133).
- **Progress.** The bar moves through each step (partitioning, encrypting, copying, your
  account, the boot loader) and names it (#134).
- **Failures.** A failed install says why on the installer screen and in the system log, with
  passwords removed (#135).
- **Tests.** The install test goes through the installer session's own code, from the broken
  state rc14 left; new unit tests cover the session, the screens' backend and the install core.

## 0.1.0~rc15 · The first online update

rc15 is the first release delivered as an update: an rc14 laptop finds it under Settings →
Updates and installs it as a new generation, keeping rc14 to go back to. It carries only fixes
to the installed system; the installer fixes found while installing rc14 on the HP Stream
(#132–#139) follow in rc16, so install from the rc14 stick and update.

- **sudo.** The writer administers the laptop with `sudo` and their own password, as on Debian;
  root stays locked and nothing is passwordless (#142).
- **Wi-Fi.** Joining or forgetting a network no longer fails with a polkit-agent-helper error:
  the writer's session may change Wi-Fi without an admin prompt, as on Debian (#141).
- **Google Drive from a phone.** The release build carries WrOS's Google sign-in clients again,
  so Backups offers linking Drive by scanning a QR code with a phone (#140).
- **The update channel.** `nixos-updates/` is published on GitHub Pages beside the Debian
  `updates/` channel and the phone-link pages, in one deploy that carries all three or none.
  The update is a delta from rc14: only what changed is downloaded.

## 0.1.0~rc14 · WrOS on NixOS

WrOS now runs on NixOS 26.05 (`flake.nix`, `nixos/`) instead of Debian 13. Every rc13 feature
is kept. Install it fresh from the USB stick: an rc13 machine can't update to rc14 in place, so
back up, install, then restore your books from the backup.

- **The system.** The installed system is declared in `nixos/modules/`, and each update installs
  a whole new generation beside the current one. Going back is going to the previous
  generation. Updates are signed offline bundles, staged and applied at a restart, and refused
  if older, the same, or tampered with. Repair re-activates the current generation.
- **The installer.** It encrypts the disk (LUKS2) and installs offline from the stick. Before
  it erases a disk it re-checks that the disk is the one chosen and that nothing on it is in
  use. It makes the disk bootable only as its last step, after the writer account exists, so an
  install interrupted part-way leaves a disk that won't start rather than a broken WrOS (#3).
- **The look.** The boot menus (installed system, and the stick's UEFI and BIOS menus) show
  the WrOS logo in the theme's colours. The system names itself WrOS and its version
  throughout. The menu bar and status line sit on a band of the theme's border colour. The
  word-goal meter is a line, as the music player's is.
- **The start screen.** The bar follows the boot as it happens: it reads systemd's progress
  directly, without waiting for D-Bus. It is exactly the logo's width and shares its centre.
- **Settings.** A long grey notice wraps inside the box instead of stretching it across the
  screen.
- **The live session.** The graphical session no longer quits a few seconds after starting
  from the stick (#129).
- **Fixes.** The phone-link QR page's clicks (#124); no menu loses a row silently (#125);
  spelling "next" wraps to an earlier word in the same paragraph (#126); Find anything shows
  the outline's key (#127); the Wi-Fi mark without `/proc/net/wireless` (#128).
- **Qualification.** Eight NixOS VM tests (install, unlock and login, keyboard preview,
  console lock, recovery menu, live session, live graphical session, update and go back) and
  the Python and Rust suites under Nix. See `docs/NIXOS_QUALIFICATION.md`.

- Integrate the reviewed Argon contribution (2026-11-03): read the whole manuscript
  aloud across chapters with n/p scene navigation; one pre-export check for links,
  images, covers, never-closed %% notes, review notes and continuity; a Revision
  widget on the home screen; the Creating a novel lesson; Google Drive browsing and
  copying an item into a new local folder; sound and volume, a keyboard layout
  wizard, and setting the clock by hand with a way back to network time.
  Argon's folder sync, sync on save and mobile handoff were left out: the per-book
  readable Drive sync covers them. Remaining on-screen "WritingDeck" text is WrOS.

- Add native encrypted local/Google Drive project backups through Restic/rclone,
  private recovery-key export/import, masked Google authorization, real
  restore/hash verification for every completed snapshot, snapshot browsing,
  cancellation, stale-lock recovery and safe restore into new folders.
- Integrate installer password clearing, offline defaults, LF enforcement,
  audited media publication and keyboard/power-policy hardening for release.

- Add explicit character knowledge assertions to Continuity tracker (Issue #6),
  tracking Knows, Suspects, Unaware and Believes-otherwise states before or after
  scenes. Assertions bind to reviewed explicit facts, durable document identities,
  and exact source revisions with full retained version history, filtering,
  archiving, source-opening and export isolation, without guessing from prose
  or hallucinating contradictions.
- Keep writing-session history local even when a Git remote is configured;
  remove automatic startup/save pushes and false background-backup status.
  Preserve original editor saves, session snapshots, authorship and Undo.

## WrOS development — reviewed Argon writing contribution (2026-10-30)

- Integrated Argon's continuity, milestone/variant and chapter-reading work
  with durable document identity, guarded restore, dated history, Undo and
  shared modal input protection. Added local `wros` CLI alias and bounded
  file comparison. Grimoire attribution and existing manuscript formats remain.
- Preserved the original ZIP/patch for audit. Rich-footnote rendering and
  backup/update/Wi-Fi/suspend prototypes require further integration;
  [review findings](docs/archive/ARGON_REVIEW_2026-10-30.md) distinguish those limits.

- Added native EPUB footnotes using shared bounded source validation and the
  maintained Markdown HTML renderer, preserving original writing and Word/PDF
  behavior. Same-file note/return links, real paragraphs and literal code/URLs
  are checked with Debian EPUBCheck and four-size editor workflows.
- Reviewed Argon's third contribution against current development, retained its
  writing regressions and repaired the milestone test's invented APIs/fixture.
  Backup/update/Wi-Fi/suspend planning remains outside normal product workflows.

- Bound draft comparison and restore to the exact reviewed saved version,
  fixing second-or-later draft selection. Added guarded one-passage restore
  through the existing history engine, default Cancel, complete wording review,
  preserved unrelated writing, recovery variants, dated history and editor Undo.

- Refused missing/incomplete shared document registries before old story IDs
  can be assigned to another scene. Existing story data, unavailable metadata,
  original writing and ordinary save/Undo remain intact; restore uses the
  original registry rather than guessed scene matching.

## 0.7.5 · Colour, not color (if you like)

- **Spelling in American, British, Canadian or Australian English.**
  *Esc › Settings › Spelling* steps through them, and it's remembered.
- **Now truly nothing is only in `Ctrl-K`.** *Writing tools* adds *Next TK
  or note*, *Look up the name here* (`Ctrl-O`) and the Pomodoro timer; *This
  book* adds *Save this session now*, *Compile to Markdown* and *Update the
  project map*. A test now keeps it that way.
- *Goals…*: `Esc` leaves them as they were (it used to save, and wouldn't
  close with an empty box); `Enter` saves.

## 0.7.4 · The page behaves like a page

- **Select with the keyboard.** `Shift` with the arrows, `Home`, `End`,
  `PgUp` and `PgDn` selects as it goes.
- **A word at a time.** `Ctrl-←` `Ctrl-→` (or `Alt`, or a Mac's `Option`)
  move by word, `Ctrl-↑` `Ctrl-↓` by paragraph, and `Ctrl-Home` `Ctrl-End` go
  to the top and end of the scene. `Ctrl-Backspace` and `Ctrl-Delete` delete
  a word.
- **Double-click** picks a word; a third click, the paragraph.
- **Details.** `i` in the outline (or *Esc › Writing tools › Scene details…*)
  sets a scene's status, POV, synopsis and word target, and whether it goes
  in exports. On a notebook note it sets the other names it's known by.
- On a Mac, `Option-←` no longer types a "b" into the prose, and no `Alt`
  shortcut types its letter by accident.
- On Windows, characters made with `AltGr` (`@`, `€`, `ł` on many keyboards)
  are typed again instead of ignored.
- A click or the mouse wheel over a box (the menu, the corkboard, `Ctrl-K`)
  no longer reaches the panes underneath; the wheel moves through the list.
- `Delete` in the outline sends the highlighted item to the Trash, like `d`.

## 0.7.3 · Paste, select all, italics, and a way back out of the Trash

- **`Ctrl-V` pastes**, from the computer's clipboard, into the page or into
  any box you're typing in. `Ctrl-A` selects the whole scene.
- **Italics and bold.** `Ctrl-I` and `Ctrl-B` put them on the selected words
  or the word at the cursor, and take them off again. Both are in
  *Esc › Writing tools* too, for terminals that send `Ctrl-I` as `Tab`.
- **Restore from the Trash.** `u` on something in the Trash puts it back in
  its old folder under its old name (making the folder again if it's gone).
  Also *Esc › This book › Restore from the Trash* and `Ctrl-K`. The outline's
  hints say so whenever you're in the Trash.

## 0.7.2 · A corkboard you can change, and nothing hidden

- **The corkboard does more.** `n` adds a card (a new scene) to the
  highlighted card's chapter, `r` renames one, `d` sends one to the Trash
  (asking first), and `<` `>` move a card earlier or later. The board comes
  back after each, on the right card. Its help page now says what a
  corkboard is for.
- **Nothing is only in `Ctrl-K` any more.** *Esc › This book* now has the
  corkboard, moving up and down, scene history, searching the whole book and
  writing sessions. *Esc › Writing tools* has find & replace and the name
  check. The page's key hints add `Ctrl-F` find and `Ctrl-T` notes.
- The corkboard says "other parts" (or acts, or books) in the book's own word.

## 0.7.1 · Find the corkboard

- The outline's key hints now include `b corkboard`, and *Esc › This book*
  has a *Corkboard…* row. It was only in `Ctrl-K` before.

## 0.7.0 · Hear it, shape it, set your goals

- **Read aloud.** *Esc › Writing tools › Read aloud* reads the scene out loud
  from the sentence you're on and highlights each sentence as it goes.
  `Space` pauses, `←` `→` go back or forward a sentence, `Esc` stops with the
  cursor where you stopped. Notes are never read out. It uses a natural Piper
  voice if one is installed, or the computer's own (the Mac and Windows
  voices, Speech Dispatcher or eSpeak on Linux), and nothing leaves your
  computer. *Settings › Reading voice…* picks another.
- **Book shapes.** *Start a new book…* now starts a **novel**, a **short
  story** (three chapters: a beginning, a middle and an end) or a **blank**
  book; `Tab` picks. From a terminal: `grimoire new --short-story <dir>`. The
  classroom lessons now start with a short story, so there's nothing to
  delete on Day 2.
- **Goals.** *Esc › This book › Goals…* sets the book's word goal and a
  day's, by typing or from a preset: a month of drafting (50,000 words, 1,667
  a day), a short story, or a novel. `g` on the Progress page gets there too.
- WAV files now play in the music player (Jellyfin and Plex).
- A draft brought in that's already past 80,000 words aims for the next ten
  thousand.

## 0.6.0 · Bring your draft, see your progress

- **Bring in a draft you already have.** A Word file, Markdown or plain text
  comes in as a new book, split into chapters at its headings and scenes at
  its scene breaks, with italics and bold kept. A title page or anything else
  before the first chapter goes into Notes, and every word comes across.
  *Esc › Open another book… › Bring in a draft…*, a draft's path typed into
  *Start a new book…*, or `grimoire import draft.docx` from a terminal.
- **Progress.** Click the word count at the bottom left, or *Esc › Writing
  tools › Progress…*: your words each week for the last eight weeks, your pace
  lately, and roughly when you'll reach your goal at that pace. It cheers you
  on and never nags: no streaks, no days in a row. Grimoire keeps the record
  from now on, one line a day in the book's hidden `.grimoire` folder.
- **Packages for Debian, Ubuntu and Fedora.** Each release now comes with a
  `.deb` and an `.rpm` for x86-64 and ARM, handy for school labs. On Windows,
  `grimoire.exe` no longer needs the Visual C++ runtime.
- A twenty-second demo at the top of the README and the website, drawn by
  Grimoire itself.

## 0.5.9 · New books on macOS and Windows

- A new book started from the menu now says it's ready on macOS and Windows
  too, and the new-book box keeps a long folder path inside its edges.

## 0.5.8 · A calmer menu, and new books from inside

- **The menu is short now.** `Esc` opens nine rows instead of two dozen. The
  rest sits one step down in small groups (*This book*, *Writing tools*,
  *Settings*, *Help & about*), and `←` or `Esc` comes back out.
- **Every row says what it does.** Whichever row is highlighted gets a line
  of plain explanation underneath, and `?` opens the help page for it. So
  *Turn echo words on* now tells you it lights up a word you've used again
  too soon.
- **Start a new book from inside Grimoire.** *Esc › Start a new book…* asks
  for a name, shows where it'll live, saves and puts away the book you're
  in, and opens the new one. No terminal needed.
- **Switch books.** *Esc › Open another book…* lists the books you've had
  open, newest first, or opens one from its folder. Each is in `Ctrl-K` by
  name too. Music, the Pomodoro and your theme carry across.
- Two new help pages: *Your books*, and *The menu, row by row*, which goes
  through every row.

## 0.5.7 · Say hello to Catfinity

- *About Grimoire…* now mentions the studio's other work: Catfinity Studios
  makes **Catfinity**, an idle cat RPG for iPhone and Android. `c` (or a
  click on its line) opens its page. `grimoire --version` mentions it too.

## 0.5.6 — grimoiretui.com

- Grimoire has its own home now: **grimoiretui.com**. *About Grimoire…*
  and the installers point there, and the old address forwards to it.

## 0.5.5 — The license, in plain words

- **License** is a new help topic: Grimoire is MIT, so commercial use is
  welcome, credit is required (keep the copyright notice and license with
  it), your books are entirely yours, and a donation is appreciated but never
  required. `l` in *About Grimoire…* opens it, and so does `Ctrl-K` ›
  *License*.
- *About Grimoire…* says the same in two lines.

## 0.5.4 — Help, About and Donate

- **Help inside the app**: 24 topics explaining every feature, from getting
  started to compiling and sync. `F1` opens it anywhere, `?` wherever you
  aren't typing, or *Help…* in the menu and `Ctrl-K`. It opens on the topic
  for where you are, and typing searches every topic.
- `F1` now opens Help; `Esc` is the menu.
- **About Grimoire…** says which version you're running and that it's made by
  Catfinity Studios, with a link to catfinity.com. `grimoire --version` says
  the same.
- **Donate…**: Grimoire is free and a donation is never required, but it's
  appreciated and goes toward Grimoire's development — the box opens the
  Ko-fi page.
- Writing sessions: `l` opens a session's changes, like `→`; `h` no longer
  does.

## 0.5.3

- **The Rainbow theme's colours flow**: the spectrum across its borders, titles
  and progress bar glides round the wheel every twelve seconds (it used to
  drift so slowly it looked still).
- Fixed on a Mac: a scene with an accented name listed each saved version
  twice in its history.

## 0.5.2 — a paperback and a proper ebook

- **Paperback** (Export ▸ Paperback, or `grimoire export --paperback --trim
  6x9`): a print-ready interior — a DOCX for Word, and a PDF when LibreOffice
  is installed. Trim 5×8, 5.25×8, 5.5×8.5 or 6×9, mirrored margins with KDP's
  gutter for the page count, author and title running heads, every chapter on
  a right-hand page with a small-caps lead-in (or a drop cap), justified and
  hyphenated text, an ornament between scenes, a title and copyright page.
  `[paperback]` in `novel.toml` sets the font, size, ornament and drop caps.
- **EPUB**: a cover (`cover.jpg` in the book), a contents page, `[ebook]`
  metadata (language, ISBN, publisher, description, subjects, series) and back
  matter. EPUBCheck-clean.
- **Back Matter**: a new section for Acknowledgements, About the Author and
  Also By, per edition like Front Matter.
- The paperback and the ebook are compiled exactly like the manuscript: no
  empty chapters, curly quotes and real dashes, TKs kept and warned about, a
  Prologue's own heading, samples.

## 0.5.1 — compiled for submission

- **Export compiles a submission manuscript in standard manuscript format**
  — William Shunn's modern standard: a title page with your contact block,
  byline and rounded word count; "Surname / KEYWORD / page" on every page
  after it; parts and chapters a third of the way down, a part sharing its
  first chapter's page; `#` between scenes; END.
- **Manuscript look…**: Modern (Times) or Classic (Courier, underlined
  italics, straight quotes), Letter or A4, double or 1.5 spacing, five
  chapter-heading styles, flush or indented first paragraphs, END / THE END /
  none, and the header keyword.
- **Author details…**: legal name, byline, address, phone, email and agent,
  for the title page.
- **Samples for agents**: chapters X–Y, the first N chapters, or whole scenes
  up to N words — the title page still gives the whole book's count.
- **PDF**, through LibreOffice when it's installed.
- **Typography in every export**: curly quotes and apostrophes, real dashes and
  ellipses — your files are left as you typed them.
- **Nothing blank**: empty scenes, chapters and parts are left out, and `#`
  only ever sits between two written scenes.
- **TKs stay visible** — they were dropped mid-sentence — and export lists
  where they are first.
- **A Prologue or Epilogue keeps its own heading.**
- Fixed: reordering sections could reduce an unreadable `novel.toml` to one
  line.

## 0.5.0 — safe in every sync service

A complete pass over how a book behaves in pCloud, Google Drive, Dropbox and
Box (and OneDrive, iCloud Drive, Syncthing), checked against each service's
documented behaviour and tested on a real pCloud Drive.

- **Conflict copies from every sync service are recognised** — Dropbox's
  "conflicted copy", pCloud's "(conflicted)", Box's "(1)", OneDrive's device
  suffix, iCloud's "2", Syncthing's "sync-conflict", Google Drive's variants,
  and Grimoire's own — named in the tree ("Dropbox copy"), never counted or
  exported twice, and moved with their scene. `⚠ N conflicts` shows in the
  status bar until you settle them.
- **Settle conflicts…** puts both versions side by side: take the copy, keep
  the scene, or keep both. One `Ctrl-Z` undoes it; nothing is ever deleted.
- **A file your sync service hasn't downloaded, or can't deliver offline,
  never stops a book opening.** It's shown as *offline*, never saved over, and
  read once it arrives. An online-only placeholder (empty, or iCloud's stub) is
  never taken for a scene cut to nothing or deleted.
- **A file that blinks** while a service replaces it, or that is still
  arriving, is no longer sent to the trash or taken in half-written.
- **Saves never write a file in place**: a temp name every service ignores,
  retried while another program holds the file, and the words kept in
  recovery until it succeeds.
- **Every name Grimoire makes is safe on every service and system** — no
  characters Windows forbids, no reserved names, nothing too long, never two
  names that differ only by capitals or accents. Pairs like that already on
  disk are marked *name clash*.
- **Writing-session history lives on each machine, outside the book**, where
  no sync service can damage it; an older book's history can be moved out.
- **The phone never saves over words** that arrive just after its own save,
  and moving its shelf never loses a file that changes mid-move.
- Grimoire says which sync service a book is in.
- Fixed: an unreadable `novel.toml` could be replaced by a one-line file at
  launch; a scene that became unreadable could spawn a conflict copy every two
  seconds.

## 0.4.7

- **Custom themes mix and match**: under the swatches, choose any theme's
  Pomodoro world and any theme's Visualizer (`←` `→`), previewed live in the
  pane — Kanagawa's great wave with Synthwave '84's neon grid, say.
- **Rainbow's Pomodoro is redrawn**: five fine ribbons curving in braille,
  standing in two soft clouds, a light running slowly along the outer band by
  day — instead of chunky blocks.
- **Tokyo Night's Visualizer is a neon city**: night-blue towers with warm and
  cool windows set into them, a neon roofline on each, a mast with a slow red
  light on the tallest, neon smeared on the wet street.
- The spelling popup is wide enough for its own hint.
- **The README has screenshots**, drawn by Grimoire itself.

## 0.4.6

- **The garden is gone.** The pane under the tree has two views, the
  **Pomodoro** and the **Visualizer**; `←` `→` switches between them.
- **Every theme has its own Pomodoro world**: a wizard's tower for Grimoire, a
  castle and bats for Dracula, snowy peaks and the aurora for Nord, cats on a
  fence for Catppuccin, the great wave for Kanagawa, a neon horizon for
  Synthwave '84 (its striped sun is the clock), a rainbow over the hills for
  Rainbow — nineteen in all. Lost Forest and custom themes keep the glade.
- **The ground fills in behind the sun** as a session goes, and a paused sun
  waits, dimmed.
- **The Visualizer looks different in every theme** too — meters, beads,
  dotted frost, a lit skyline, a synthwave grid, a rolling wave, a mirrored
  prism — and custom themes keep the original.
- **It always names the song**, cut to fit, in its title or written along the
  bottom and lit as the song plays.
- **Calmer**: a gentler beat, fewer stray peak marks, and a quiet resting line
  with the reason when nothing's playing.

## 0.4.5

- **Volume steps evenly**: each `+` or `-` moves it about ten (100 → 90 → 80),
  where YouTube Music used to lurch 100 → 74 → 55 because the step was taken
  on the scale it accepts rather than the one it shows.
- **Below full volume, the music pane shows the level** (`70%`) beside the time.

## 0.4.4

- **Click a misspelt word** (or right-click it) for its fixes, right under the
  word, in the word's own capitals — or tell Grimoire to stop calling it one:
  for now, always in this book, or always in every book. Typing anything else
  just carries on writing.
- A spelling fix undoes with one `Ctrl-Z`.

## 0.4.3

- **The music pane takes every one of the player's keys** when it's
  highlighted: `[` `]` change track, `←` `→` seek ten seconds, `space`, `r`
  repeat, `s` shuffle, `+` `-` volume, `l` like. Its edge names them while it
  has focus.
- **Repeat, shuffle, volume and like are on screen**: under the player's
  progress bar, and as `↻` `↻1` `⇄` `♥` beside the pane's time. Before, `r` and
  friends worked but nothing showed it, so they looked broken.
- **Every key says what it did** — "repeat: one", "shuffle on", "volume 100 —
  as loud as it goes" — read back from the player once it has applied the
  change, and a command the player refuses now says why instead of vanishing.

## 0.4.2

- **A spellbook heads the tree**: an open grimoire, light rising from its
  spine, sparkles twinkling above it — in your theme's colours, and in every
  colour under Rainbow. It steps aside on short windows so the outline keeps
  its rows.
- **Focus mode says where it is**: `^D focus mode` sits right after `Esc menu`
  in the status bar, and the idle timer's title reads `F2 timer · ^D focus
  mode`. The running timer now says `writing · 18:04` rather than `focus`, so
  "focus" only ever means focus mode.

## 0.4.1

- **Ten new themes**: Rosé Pine, One Dark, Monokai, Kanagawa, Ayu Mirage,
  Night Owl, Material Palenight, Synthwave '84, GitHub Dark — and **Rainbow**,
  where everything drawn in the accent (focused borders, titles, the progress
  bar, the word count) is a slowly drifting spectrum, the spectrum view's bars
  climb from red to violet, and the clearing is every colour at once.
- **The theme picker shows each theme's colours** beside its name, and scrolls
  on a short terminal.

## 0.4.0

### Keeping every word

- **Changes made elsewhere are never overwritten.** If Obsidian, a sync app or
  the phone changes a scene while Grimoire is open, Grimoire shows the new
  version; if you had unsaved words in it too, they're kept beside it as
  "Scene (from bazzite, …)", marked *parked copy* in the tree. Scenes added,
  moved or deleted outside Grimoire show up by themselves; a deleted scene is
  never brought back, and any unsaved words from it go to the trash.
- **One file that isn't UTF-8 no longer stops a book opening.** It's shown
  read-only and never rewritten.
- **The recovered-words box never throws words away on a keystroke:** choose
  with ↑↓ and `Enter`. "Keep the saved versions" puts the recovered words in
  the trash rather than deleting them.
- **Moving a section right after typing** no longer loses the last few
  seconds of writing.
- **Two things with the same name deleted in the same second** both stay in
  the trash.
- **A move that can't finish** (a locked folder, a sync app holding a file)
  puts everything back, and anything a crash left mid-move reappears in the
  tree next time the book opens.
- **Undoing a move keeps its link fixes**, and a new scene no longer inherits
  a deleted scene's undo or history.
- **Find and replace match whole words in their exact case** by default —
  `Ctrl-W` for inside words, `Ctrl-E` for any case — and one `Ctrl-Z` undoes a
  replace across the whole book.

### Writing

- **Focus mode** (`Ctrl-D`): just your prose, centred, with a quiet status
  line. `Esc` still opens the menu; `Ctrl-D` again brings everything back.
- **A readable line length**: prose wraps at 72 columns, centred. *Line
  width* in Settings changes it (60 / 72 / 80 / 100 / full).
- **Typewriter scrolling**: in focus mode the line you're writing stays
  mid-screen; elsewhere you're never drafting on the bottom row.
- **Notes that never reach the book**: `%% like this %%` (Obsidian's comment
  syntax, may cross lines) and TK — dimmed in the editor, left out of every
  word count, compile and export, kept on disk exactly as typed. **`Ctrl-T`**
  lists every note and TK in the book; type to filter, `Enter` goes there.
- **Sprints**: *Start a sprint…* sets a word goal and minutes, starts the
  timer, and the status bar counts `sprint 212/500 · 14:32`. A scene's
  `target:` shows as progress in the editor's title.
- **Revision passes**: *Next scene still in draft*, and *Echo words* (the same
  word again within forty).
- **A scene beside**: `v` in the tree shows another scene, read-only, next to
  the one you're writing. `Esc` closes it; `Enter` switches to writing in it.
- **An empty scene says how to start.**

### Around the desk

- **The Esc menu has the new tools**: focus mode, a scene beside, notes &
  TKs, next scene in draft, echo words and sprints; Settings has line width
  and typewriter scrolling. Rows that can't do anything right now are hidden,
  and the menu scrolls on a short terminal. From the editor, the menu's
  New/Rename/Delete act on the scene you're writing, as `Ctrl-K`'s always did.
  Lists answer PgUp/PgDn and Home/End throughout.
- **Export… is in the menu** — Word, EPUB or Markdown from one dialog, which
  now fits an 80-column terminal. Compile and the project map are still in
  `Ctrl-K`.
- **A new book opens on its Novel Format guide**, written as plain prose.
- **New books count in parts again** (Part One, `p` new part). Older books
  keep their folders' names — a book an earlier version switched to pages
  keeps its pages, and one with its own label (Act, Book) is untouched.
- **The spectrum works on Linux**: it hears whatever the machine plays through
  PipeWire (`pw-record`) or PulseAudio (`parec`), only while the view is
  showing; with neither installed it says so instead of sitting blank.
  `GRIMOIRE_MONITOR=<sink>` listens to a different output.
- The menu leaves out *Music player* while music is off; history compares each
  version with now ("6 words shorter"); resuming an empty scene says it was
  left open; corkboard cards without a synopsis show the scene's first line; a
  character note's title names its section.

### Under the hood

- The book's logic lives in `grimoire-core`, shared with the phone app, which
  now builds from the same repository.
- Tests run on Linux, macOS and Windows for every change, with clippy and
  rustfmt enforced.

## 0.3.6

- **`Esc` closes an open character note** whichever pane you're in — 0.3.5
  promised this but only did it with the note itself focused; from the editor
  it opened the menu and left the note up. `Ctrl-O` on the same name (or on
  none) now puts the note away too.
- **The status bar leads with what `Esc` does**, so an 80-column terminal no
  longer cuts "Esc menu" off the end.

## 0.3.5

- **`Esc` opens the menu**, from any pane — no more reaching for `F1`, which
  many keyboards only send with `Fn` held (it still works). `Esc` again closes
  it and puts you back where you were, so it no longer drops you from the
  editor into the tree; `Shift-Tab` does that. A selection in the editor, or
  an open note beside it, is closed by the first `Esc`.
- **Quit Grimoire** is in the menu, at the bottom. It saves everything first,
  like `Ctrl-Q`.

## 0.3.4

- **"today" always moves when you write.** It's the day's net change in the
  manuscript, shown below zero (`today −29`) after cutting more than you've
  written, instead of sitting at 0 until you'd written the cut words back.
  Deleting a scene or bringing it back with `Ctrl-Z` doesn't change it, cut and
  paste within the book comes out even, and it starts over at midnight even if
  Grimoire stays open.

## 0.3.3

- **Closing the terminal window really closes Grimoire.** It used to be left
  running, stuck at full CPU on the closed terminal (a crossterm reader that
  never returns), without the save-on-close ever happening. Keys are now read on
  their own thread, so closing the window saves and exits in a moment.

## 0.3.2

- **Everything in the tree moves**: drag a section, or press `K`/`J` on it, to
  reorder the sections (saved in `novel.toml`; the trash stays last). Notes
  without a number in their filename can be moved too — the folder is numbered
  on the first move. All of it undoes with `Ctrl-Z`.
- **A shorter menu**: themes, music source, music, spellcheck and tree icons
  are in `F1` → **Settings…**, which opens a menu of its own.

## 0.3.1

- **Undo in the tree**: `Ctrl-Z` outside the editor takes back a delete,
  rename, move (a whole drag at once) or new item; `Ctrl-Y` does it again. A
  deleted item comes back out of the trash to where it was. Typing is still
  undone from the editor.
- **Tree icons are optional** and start off: F1 → *Turn tree icons on*.

## 0.3.0

- **A tree in sections**: Novel Format, Manuscript, Characters, Places, Front
  Matter, Notes, Research, Template Sheets and Trash, each with its own symbol,
  every section and folder foldable, headed by the book's title.
- **Pages** are the book's largest division (they were parts): `p page`.
- **Front matter by edition** — Manuscript Format, Paperback, Ebook — with
  starter title, copyright and dedication pages, left out of exports until
  you include them. The EPUB takes the Ebook pages; Word takes the Manuscript
  Format ones.
- **Template Sheets** for a character and a setting, and a Novel Format guide.
- **Older books are upgraded once, on opening**: notebook folders move into
  their sections, parts become pages, links follow.
- Popup keys stand out in the accent colour; the clearing pane has a view
  switcher (`◂ clearing spectrum garden ▸`); the garden is titled.

## 0.2.0

Nothing written is one keystroke from gone, and a hundred scenes are easy to
find and shape.

- **Autosave**, two seconds after you stop typing, through temporary files so
  a crash can't tear a scene; closing the window or quitting saves first.
- **Recovery**: words that couldn't be saved are offered back next launch.
- **Undo and redo**, a word at a time; typing replaces a selection; paste is
  one step; Ctrl-X cuts.
- **Scene history** (`H`): dated versions with a word diff; restore any.
- **Command palette** (`Ctrl-K`): every action, scene, note and theme by name.
- **Find and replace** in a scene (`Ctrl-F`) or across the book (`/`), with a
  confirm before replacing across scenes.
- **Name drift**: near-miss spellings of notebook names, fixed in one go.
- **Spellcheck** with a bundled en_US dictionary that knows your notebook's
  names; `F8` for suggestions; words you add go in `dictionary.txt`.
- **Move scenes and chapters** (`Alt-↑/↓`, `K`/`J`, or drag in the tree),
  across chapters and acts, with `[[links]]` kept pointing at the right files.
- **The corkboard** (`b`): index cards with POV colours, status, synopsis and
  words against target; filter by POV; edit on the card.
- **The codex** (`Ctrl-O`): a notebook note beside the scene, with every scene
  that mentions it.
- **Export for readers**: Word (standard manuscript format), EPUB and Markdown,
  whole book or chosen acts; also `grimoire export`.
- **Resume** where you left off, on any computer the book reaches.
- **Writing sessions**: optional Git history labelled like a diary, with
  background backup to a remote.
- **Music is off by default**; turn it on from the menu or palette.

## 0.1.0

The first release, and the first with ready-to-run downloads — no Rust
toolchain needed.

Built for Windows (x64), macOS (Apple Silicon and Intel) and Linux (x86_64 and
ARM64); the one-line installers are below.

**What's in it**

- A manuscript that is just folders and Markdown files — the directory tree is
  the outline. New books start as three acts of nine chapters with three scenes
  each, plus a notebook and a visible trash.
- Create, rename and delete parts, chapters and scenes from the tree; deleting
  moves things to the trash rather than destroying them.
- Word counts and daily progress, a project map, and `grimoire compile` to
  assemble the manuscript.
- Music while you write: YouTube Music, Spotify (macOS and Linux), Jellyfin and
  Plex. `grimoire music-setup youtube-music` installs and connects YouTube Music
  on all three systems.
- Themes, switched with `F9`.
