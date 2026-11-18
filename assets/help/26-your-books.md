# Your books

Every book lives in a folder of its own, and you can have as many as you like. Starting one, or hopping between them, is a couple of keys.

## Starting a new book

*File › Start a new book…* (or type *new book* in `Ctrl-K`).

Type a name and press `Enter`. That's all. `Tab` first picks what it starts as:

- **Novel**: three parts of nine chapters, three scenes in each, aiming for 80,000 words.
- **Short story**: three chapters (a beginning, a middle and an end), three scenes in each, aiming for 5,000 words.
- **Blank**: one chapter with one empty scene, to build your own way.

Whichever you pick, you can add, rename and delete anything later. The book you had open is saved and put away first, and the new one opens on a page that shows how a book is laid out. It comes with parts, chapters and scenes already in place, ready to fill in, rename or delete.

A plain name goes in a folder beside the book you had open. With *~/Documents/Grimoire* open, *The Long Night* becomes *~/Documents/The Long Night*. The box shows exactly where before you press `Enter`, and a path (like `~/Writing/The Long Night`) puts it anywhere you like.

If there's already a book by that name, `Enter` opens it instead of making a second one. A folder that already has other things in it is left alone, so try another name.

From a terminal, `grimoire new <folder>` starts one too, with `--short-story` or `--blank` for those shapes.

Got a draft already? See *Bringing in a draft*, just below.

## Bringing in a draft

Already have a draft somewhere else? Bring it in and it becomes a WrOS book, split up and ready to work on. WrOS reads a **Word file** (`.docx`), **Markdown** (`.md`) or **plain text** (`.txt`). From Google Docs, *File › Download › Microsoft Word* first.

Choose *File › Open another book… › Import a book…*, then *A draft (Word, Markdown, text)*. Browse from your home folder with the arrow keys and `Enter`. Folders open; pick a draft file to make it a book. *Up one folder* takes you back. From a terminal, `grimoire import <file>` works too.

Here's how it's split up:

- **Chapters** start at headings. In Word that's the *Heading 1* (or *Heading 2*) style; in Markdown it's `#` or `##`. With no headings at all, a line on its own like *Chapter Three* or *Prologue* counts.
- **Parts** are headings that start with *Part*, *Book* or *Act* and a number, like *Part Two*.
- **Scenes** start at a scene break: `***`, `* * *`, `#` or `~` on a line of its own. A heading one level below the chapters starts a titled scene.
- *Italics* and **bold** come across.
- Anything before the first chapter, like a title page, is kept in **Notes**, so it doesn't turn into a chapter.
- An *END* on its own at the very end is left off, since WrOS's export adds its own.

The draft itself is never changed. The new book goes in *~/Documents*, named after the draft file, and opens straight away. If that folder name is taken, WrOS adds a number. It tells you how many chapters, scenes and words came across, and every word lands somewhere.

## Switching books

*File › Open another book…* lists books in *~/Documents*, with the ones opened lately first, then by title. It also keeps books you've opened elsewhere. The book you're in says *open now*. Pick another and press `Enter`: this book is saved and closed, and the other opens right where you left off.

- *Import a book…* brings a copy into *~/Documents* from Google Drive, a USB stick or a folder on this laptop. Pick folders with the arrow keys and `Enter`, then pick a book. The copy is checked before it opens; the source stays as it was. Drafts come in here too.
- *New book…* starts a book of your own.
- In `Ctrl-K`, type part of a book's name. Every book on the list is there as *Open the book …*.

Your music keeps playing, the Pomodoro keeps counting, and the theme stays just as it is.

## Deleting a book

In *Open another book…*, move to a book and press `x`. A box names the book and its folder. Choose *Delete it* with the arrow keys and press `Enter`; `Esc` or *Keep it* leaves it alone. For a book of your own you also type its folder's name, so it can't happen by accident. A tutorial demo book needs only *Delete it*.

Deleting removes the book's folder and everything in it from this laptop, and it can't be undone here. Backups of it, and any copy on Google Drive, are kept. The book you have open can't be deleted: open another one first.

## Which book opens when WrOS starts

`grimoire` on its own opens the book in the folder you're in, if you're in one. Otherwise it opens the last book you had open, so after a switch that's the one you switched to.
