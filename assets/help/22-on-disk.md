# On disk

A project is an ordinary folder you can open, copy and back up without WrOS.

## The folder

| Path | What it is |
|---|---|
| `project.toml` | the title, author, word targets, and export details |
| `Project-Format.md` | the guide to how the project is laid out |
| `manuscript/` | the story: parts, chapters and scenes |
| `characters/`, `places/` | the notebook |
| `notes/`, `research/` | more of the notebook |
| `front-matter/`, `back-matter/` | pages before and after the story, by edition |
| `template-sheets/` | Character and Setting sketches to copy |
| `dictionary.txt` | words spellcheck accepts in this project |
| `cover.jpg` | the EPUB's cover, if you add one |
| `exports/` | what Export writes |
| `project.md` | a map of the project, if you ask for one |
| `.grimoire/` | history, recovery, the Trash, and where you left off |
| `.grimoire/project-id` | the ID used to find the project's writing history |

Folders are containers, at any depth, and `.md` files are scenes or notes. A leading number like `01-` sets the order and is hidden in the outline.

Before rc22, `.grimoire/project-id` was `.grimoire/book-id`, and `Project-Format.md` was `Novel-Format.md`. WrOS still reads the old names and renames the files when it opens the project. The guide is titled *Project Format* in new projects.

## Two files made on request

Both are in `Ctrl-K`:

- *Update project map* writes `project.md`: every part, chapter and scene, with word counts and a link to each file, in a marked block WrOS rewrites each time. Anything you write above or below the block is left alone.
- *Compile manuscript (Markdown)* writes the whole project as one Markdown file, `title-manuscript.md` beside `project.toml`, through the same compile as Export.

## A scene's frontmatter

The lines between `---` at the top of a scene:

- `title` — its name in the outline.
- `pov`, `status`, `synopsis` — shown on the corkboard. Statuses are idea, outline, draft, revised and done.
- `target` — the scene's word target.
- `compile: false` — keep it in the outline but out of exports.

A note can have `aliases` too. WrOS only changes the one line it means to, so Obsidian and anything else can keep fields of their own there.

## project.toml

`title`, `author`, `target_words`, `daily_target`, and `part_label` (what the project calls its parts: Part, Act, Book). Below them, tables WrOS writes for you: `[contact]` (author details), `[manuscript]` (the manuscript's look), and optionally `[ebook]` and `[paperback]`.

Before rc22, this file was `novel.toml`. WrOS still reads that name and renames the file to `project.toml` when it opens the project.

## Your settings

Kept in `~/.config/grimoire/`: `settings.toml`, `theme.toml`, `music.toml`, and `dictionary.txt`, your spelling list for all projects.
