# Your projects

Every project lives in a folder of its own, and you can have as many as you like. Starting one, or hopping between them, is a couple of keys.

## Starting a project

*File › New project…* (or type *new project* in `Ctrl-K`).

Type a name and press `Enter`. That's all. `Tab` first picks what it starts as:

- **Novel**: three parts of nine chapters, three scenes in each, aiming for 80,000 words.
- **Short story**: three chapters (a beginning, a middle and an end), three scenes in each, aiming for 5,000 words.
- **Essay**: Introduction, Body and Conclusion in a flat manuscript, aiming for 3,000 words. Press `n` in the outline to add a section.
- **Blank**: an empty manuscript, to build your own way.

You can add, rename and delete draft items later. WrOS saves the open project before switching. Novel, Short story and Essay include a **Project Format** guide.

A plain name goes in a folder beside the project you had open. With *~/Documents/Grimoire* open, *The Long Night* becomes *~/Documents/The Long Night*. The box shows exactly where before you press `Enter`, and a path (like `~/Writing/The Long Night`) puts it anywhere you like.

If there's already a project by that name, `Enter` opens it instead of making a second one. A folder that already has other things in it is left alone, so try another name.

From a terminal, `grimoire new <folder>` starts a novel. Use `--short-story`, `--essay` or `--blank` to choose another shape.

Got a draft already? See *Bringing in a draft*, just below.

## Bringing in a draft

Already have a draft somewhere else? Bring it in and it becomes a WrOS project, split up and ready to work on. WrOS reads a **Word file** (`.docx`), **Markdown** (`.md`) or **plain text** (`.txt`). From Google Docs, *File › Download › Microsoft Word* first.

Choose *File › Open another project…*, then *Import a project…* and *A draft (Word, Markdown, text)*. Browse from your home folder with the arrow keys and `Enter`. Folders open; pick a draft file to make it a project. *Up one folder* takes you back. From a terminal, `grimoire import <file>` works too.

Here's how it's split up:

- **Chapters** start at headings. In Word that's the *Heading 1* (or *Heading 2*) style; in Markdown it's `#` or `##`. With no headings at all, a line on its own like *Chapter Three* or *Prologue* counts.
- **Parts** are headings that start with *Part*, *Book* or *Act* and a number, like *Part Two*.
- **Scenes** start at a scene break: `***`, `* * *`, `#` or `~` on a line of its own. A heading one level below the chapters starts a titled scene.
- *Italics* and **bold** come across.
- Anything before the first chapter, like a title page, is kept in **Notes**, so it doesn't turn into a chapter.
- An *END* on its own at the very end is left off, since WrOS's export adds its own.

The draft itself is never changed. The new project goes in *~/Documents*, named after the draft file, and opens straight away. If that folder name is taken, WrOS adds a number. It tells you how many chapters, scenes and words came across, and every word lands somewhere.

## Switching projects

*File › Open another project…* opens *Your projects*, which lists projects in *~/Documents*, with the ones opened lately first, then by title. It also keeps projects you've opened elsewhere. The project you're in says *open now*. Pick another and press `Enter`: this project is saved and closed, and the other opens right where you left off.

- *Import a project…* brings a copy into *~/Documents* from Google Drive, a USB stick or a folder on this laptop. Pick folders with the arrow keys and `Enter`, then pick a project. The copy is checked before it opens; the source stays as it was. Drafts come in here too.
- *New project…* starts a project of your own.
- In `Ctrl-K`, type part of a project's name. Every project on the list is there as *Open the project …*.

Your music keeps playing, the Pomodoro keeps counting, and the theme stays just as it is.

## Deleting a project

In *Open another project…*, move to a project and press `x`. A box names the project and its folder. Choose *Delete it* with the arrow keys and press `Enter`; `Esc` or *Keep it* leaves it alone. For a project of your own you also type its folder's name, so it can't happen by accident. A tutorial demo project needs only *Delete it*.

Deleting removes the project's folder and everything in it from this laptop, and it can't be undone here. Backups of it, and any copy on Google Drive, are kept. The project you have open can't be deleted: open another one first.

## Which project opens when WrOS starts

`grimoire` on its own opens the project in the folder you're in, if you're in one. Otherwise it opens the last project you had open, so after a switch that's the one you switched to.
