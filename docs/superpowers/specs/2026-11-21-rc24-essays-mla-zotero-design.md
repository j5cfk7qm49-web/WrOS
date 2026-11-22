# rc24: essays and MLA 9 (design)

Owner decisions from 2026-11-21 and 2026-11-22, on the rc23 desk. Calls marked "(my call)" are design proposals.
[Brain, Library and Review](2026-11-22-brain-library-review-design.md) builds on this release's project sources and picker.

## Why

The owner hands in MLA 9 papers and cites a few sources in each. They need an essay scaffold,
a quick way to cite while writing, and a Word document or PDF they can turn in.


## Goals

1. Add **Essay** to **New project…** and to the command line.
2. Add a project source list, manual entry and file import. Insert a source and optional page at the cursor. Write and export using saved sources offline.
3. Produce an MLA 9 paper in Word and PDF. Use the same qualified Hayagriva citation runs for both formats. Keep public footnotes editable in Word and at the foot of the page in PDF.

## Non-goals

- Academic paper, poetry, memoir and the rest of #163. This release needs one form to prove the essay workflow. Academic paper would add different sections and submission defaults. (my call)
- A user template loader, support for more citation styles, DOI or ISBN lookup, and live Word citation fields.
- MLA layout in paperback or EPUB. The editable Markdown routes keep Markdown.

## Anti-goals

- A new dashboard, a global shortcut grabbed from the desk, or text inserted into the wrong pane.
- An essay with a fiction title page, chapter starts, ornamental breaks or an END line.
- A clean-looking paper with a missing citation, a lost footnote, or a substituted PDF font passed off as Times New Roman.

## Approaches considered

- **One shape or a template loader.** Add an Essay case to `Template` and reuse the current scaffold's safe file creation. (my call) The current shape models parts, chapter folders and scene files. Use the existing flat manuscript reader for Essay. An Essay needs flat sections and different starter files. Prove those with one built-in shape before fixing a TOML format for every kind of project. Keep the grouped picker from #163. [Current scaffold](../../../crates/grimoire-core/src/project.rs)
- **Hayagriva or Pandoc.** Try the checked 0.9.1 Hayagriva pin first with the CSL-JSON feature and an explicit MLA 9 style. Keep the CLI and bundled style archive out of the build. Use off-device Pandoc citeproc as an independent comparison. If the adapter or processor fails the fixtures, fix the narrow fault and rerun them. A separate local processor needs a size and licence decision before it ships. The MLA label waits for passing results. (my call) [Hayagriva manifest](https://github.com/typst/hayagriva/blob/v0.9.1/Cargo.toml) · [Hayagriva library](https://github.com/typst/hayagriva/tree/v0.9.1) · [Pandoc citations](https://pandoc.org/MANUAL.html#citations)

## Decided details

### The desk

- (my call) Essay starts with three editable outline entries, **Introduction**, **Body** and **Conclusion**. They are ordered Markdown files directly under `manuscript/`. Call them sections in Essay creation and outline actions. The writer can rename, reorder and add them. With Essay's manuscript selected, **Project › New section…** and `Ctrl-K` › **New section…** use the outline's existing `n` action. Offer the section action in that context and keep the normal creation actions elsewhere. Their names are drafting aids. Only headings the writer types into the text appear in the MLA paper. Aim for 3,000 words overall. Keep the normal word count and daily goal controls. Reuse Notes and Research for claims and quotes, with **Project sources…** for references. Skip fiction starter sheets and front and back matter. Keep normal spelling and prose checks. Hide dialogue-specific prompts by default for Essay. Use a Word-first export with optional PDF. (my call)
- The grouped picker lists Novel and Short story under **Fiction**, Essay under **Non-fiction**, then **Blank**. Skip empty groups. Show one description for the selected shape. Use `grimoire new --essay <dir>`. Use the same scaffold from CLI and shelf. Leave the other forms' file layouts and defaults as they are. Write `project.toml` and `Project-Format.md`. Use the existing identity machinery for `.grimoire/project-id`. [Shelf and picker](../../../src/app/overlays/shelf.rs) · [CLI](../../../src/main.rs) · [Project names](../../../crates/grimoire-core/src/project_files.rs)

~~~text
New project…
  Name     [ Reading and attention                  ]
  Fiction       Novel / Short story
  Non-fiction > Essay
                Introduction, Body and Conclusion.
                3,000 words. MLA 9 paper.
                Blank
  [Create]                                      Esc
~~~

These rows use the existing menus and groups. `Esc` opens the bar after dismissing a selection or pane where applicable. `F10` opens it directly. `Ctrl-K` opens the palette. Keep the current menu navigation, `↑/↓`, `←/→` and `Enter`. No new global key. Expose the source list through the existing menus, within the rc23 limit of twelve rows. Keep `F11`, `Ctrl-O`, `Ctrl-F` and editor Undo as they are. Check the menu cap on the rc23 branch as well. [Menu](../../../src/app/overlays/bar.rs) · [Palette](../../../src/palette.rs) · [Settings routes](../../../src/app/shell.rs) · [Every key](../../../assets/help/24-every-key.md)

| Menu path | Ctrl-K entry |
|---|---|
| File › New project… | New project… (existing) |
| Tools › Notes › Project sources… | Project sources |
| Tools › Notes › Add a source… | Add a source |
| Tools › Notes › Import sources… | Import sources |
| Settings › Writing › Manuscript › MLA 9 paper details… | MLA 9 paper details |
| File › Export… | Export for readers (Word, PDF, EPUB) (existing) |
| File › Compile to Markdown | Compile manuscript (Markdown) (existing) |

- **Writing › Manuscript** gains **MLA 9 paper details…**. The export dialog's **Paper details…** opens the same form. The form has **Name**, **Running surname**, **Instructor**, **Course**, **Date** and **Save**. The title comes from the project's title field. Offer the project author as a starting value and let the writer enter a compound surname as written. Show an example date such as `21 November 2026` and save the paper's date as entered. A draft with missing details can still be written. Export Review names each required field that is empty. (my call)

In forms, Tab and Shift-Tab move between fields. Use the arrow keys in a focused list,
Enter to choose and Esc to return. Keep typed values after an import error and use a
scrollable one-column layout at 80×24. In Export, arrows move between rows, Space or
Enter ticks a format and Left or Right changes the layout.

~~~text
Project sources
  Find [reading                             ]
  > Baron, Naomi S. · 2013       baron2013
  [Edit…]  [Add a source…]  [Import sources…]
  Esc back

Add a source
  Type [Journal article ▾]   Key [baron2013  ]
  Title [                                  ]
  Author [Family        ] [Given           ] [Add author]
  Date [          ]   Journal [            ]
  Volume [   ] Issue [   ] Pages [         ]
  DOI or URL [                             ]
  [Save source]                         Esc cancel

Insert citation
  Project sources  Find [baron            ]
  > Baron, Naomi S. · 2013       baron2013
  Pages [194       ]   [ ] Author named in text
  [Add another source]   [Insert]
  [@baron2013, p. 194]                    Esc back
~~~


Typing @ at the start of a word in a writable manuscript or note opens this picker over
the current project's sources. Email addresses and pasted text stay literal. A second @ before
any search text inserts one literal @; Esc dismisses the picker with the document unchanged.
Search as you type, choose a source, enter an optional page or range, and use **Author named in
my sentence** for the suppressed-author marker. The picker can collect multiple sources.
Respect existing key handling, including selection, text entry, menu and modal ownership.
Use the existing footnote insertion guards as a model: capture the original document, pane,
cursor/selection and revision, recheck on acceptance, and insert into that same pane in one Undo
step. Undo removes the marker and leaves the source available.
[Footnote insertion tests](../../../src/footnote_insert_ui_tests.rs)

### Sources and project files

- `sources.json` at the project root is a UTF-8 CSL-JSON array, initially `[]` for Essay and created on first source save for older projects. Store one stable case-sensitive `id` per record. Accept safe keys made from ASCII letters, digits, hyphens and underscores. Suggest author plus year for new entries and offer a suffix on collision. Preserve valid unused import keys. Preview any repaired key and each unmapped field before import. Match a clear duplicate for reuse with confirmation. Preserve valid fields and creator order. Guard deletion of cited records and lock the key on ordinary edits. Keep a malformed file intact and show the error. (my call) [CSL-JSON item schema](https://github.com/citation-style-language/schema/blob/master/schemas/input/csl-data.json)
- Manual entry supports monographs, edited chapters, journal articles and web pages first. Use a BibTeX/BibLaTeX parser for `.bib`, including macros, accents and protected capitals. Import `.bib` and CSL-JSON through a path field and preview. Show adds, duplicates and losses before saving a batch. Preflight accepts `[@key]`, `[@key, p. 12]`, `[@key, pp. 12–14; @other]` and `[-@key, p. 12]` for author named in the prose. For a name and title already supplied in prose with no locator, allow `[-@key]` only if the fixture proves it creates a bibliography entry without empty parentheses. Fail review on recognizable unsupported syntax. Leave ordinary prose, email, code, escaped brackets, links and images alone. (my call) [BibLaTeX documentation](https://ctan.org/pkg/biblatex) · [Pandoc citation syntax](https://pandoc.org/MANUAL.html#citation-syntax) · [MLA in-text examples](https://style.mla.org/in-text-citations-overview/)
- Store the template ID and export profile in `project.toml`. (my call) Use `[mla]` for heading and running-surname fields and `[citations]` for the style ID, qualified revision and `en-US` locale. Keep the style and locale in the release. Missing metadata on an old project keeps the existing export default. Choose the MLA profile in Export for any project. Turning it on does not rearrange existing files. Resolve old project settings via the rc22 helper. Write only the new names. [Project file helper](../../../crates/grimoire-core/src/project_files.rs) · [CSL styles](https://github.com/citation-style-language/styles) · [CSL locales](https://github.com/citation-style-language/locales)
- Sort records by ID and write stable, readable JSON. Use an atomic replace and fingerprint check for local edits and imports. Use the project save and timeline hook. Root `sources.json` and `project.toml` travel with readable Drive, folder import, encrypted project backup and timeline snapshots. Show a **Project sources** file entry for timeline restore. A sources-only restore previews keys it would strand in the current text. The Drive helper already syncs visible files, while its text merge-base allowlist omits JSON. Add the exact root `sources.json` path to that bounded allowlist. Use base, desk and parked Drive copy to merge independent changes by ID. Offer a choice for two edits to one entry, delete versus edit and key collisions. Keep both copies when the base is missing or either side is invalid. Honor the sync and pending-file contract on save and resolve. Drive can deliver scenes and sources at different times. Recheck after sync and block formatted output with unresolved keys or pending conflicts in its inputs. [Sync helper](../../../os/scripts/wros-sync) · [Timeline](../../../crates/grimoire-core/src/timeline/mod.rs) · [Native backup](../../NATIVE_BACKUP.md)

- Reserve a versioned, project-local provenance slot inside each CSL-JSON record for the later
  library integration. The proposed `_wros` object carries version, work ID, edition ID and fingerprint
  of the accepted bibliographic fields. Standalone rc24 records omit it. Load, save, merge and
  restore it with the owning record and keep unknown future metadata intact. Pass only CSL fields
  to citation engines and omit the WrOS metadata from reader handoffs. See the
  [library provenance contract](2026-11-22-brain-library-review-design.md#citations-and-library-editions).

### MLA output and proof

~~~text
Export
  Layout         [MLA 9 paper  ▾]   [Paper details…]
  Formats        [x] Word   [ ] PDF   [ ] Markdown
  [Review before export] p       [Export] x
~~~

- Add a named profile, **MLA 9 paper**, beside the existing manuscript layout. The profile defaults to whole-project US Letter. (my call) Make one Word section with 1-inch margins, Times New Roman 12 pt, double spacing and half-inch first-line indents in body paragraphs. Put the right-aligned header half an inch from the top, including page one. Use the entered surname, a space and a live Word PAGE field. At the top left of page one put name, instructor, course and date on four lines. Centre the plain project title above the text. Keep the numbered section files in reading order without fiction page breaks. Have `grimoire export --docx --pdf` honor the same saved profile and checks. Start **Works Cited** on a new page with the same running head, a plain centred heading, half-inch hanging indents and double spacing. (my call) Use **Works Cited** even for one entry, per the owner's label. The MLA example guide suggests the singular for one. With zero cited sources omit the page. [MLA paper layout](https://style.mla.org/app/uploads/sites/3/2020/12/Formatting-a-Research-Paper_v3_-The-MLA-Style-Center.pdf) · [MLA running heads](https://style.mla.org/adapt-mla-running-heads/)
- Use ordinary editable runs for styled citations and bibliography entries. Reuse the actual DOCX footnote machinery for scene-local `[^label]` definitions. Give MLA footnotes 12 pt and double spacing, as in the body, with superscript numbered references and continuous numbering. Citations inside referenced notes join the same Works Cited as body citations. Resolve markers over the exact included text and notes in reading order after excluding private notes and omitted material. Scope checks and source ranges must match the export. In Review (`p`), show file and line for bad keys, broken markers or notes and unresolved conflicts. Repeat checks before Export (`x`) publishes any companion file. Export leaves the Markdown input intact. Raw **Compile to Markdown** and Markdown export retain citation markers and warn about unresolved keys. If a reader format lacks citation rendering, stop that selection and offer Word, PDF or raw Markdown. (my call) [MLA footnote formatting](https://style.mla.org/note-formatting-in-microsoft-word/) · [Existing Word notes](../../../crates/grimoire-core/src/export/docx_footnotes.rs) · [Export review keys](../../../assets/help/18-compile.md)
- Use Typst as the local PDF renderer (0.14.2 at the WrOS Nix pin) with Liberation Serif (SIL OFL).
  Word names Times New Roman and leaves the rendering to the recipient's Word installation. The
  app renders the same prepared manuscript, styled citation runs and real footnotes into both
  formats. Treat manuscript text and metadata as data, escaping them before they reach Typst.
  Run the compiler in a private temporary directory with pinned fonts and a deadline. Stage the PDF
  and replace the destination only on success. If Typst fails, report the PDF error, keep the
  useful Word output and clearly identify any older PDF. Name the outputs
  Surname_Title_MLA.docx and Surname_Title_MLA.pdf in exports/. Measure the installed closure
  and peak memory on the HP Stream and check the resulting PDF's actual embedded font.
  [Typst 0.14 changes](https://typst.app/docs/changelog/0.14.0/) ·
  [MLA paper layout](https://style.mla.org/app/uploads/sites/3/2020/12/Formatting-a-Research-Paper_v3_-The-MLA-Style-Center.pdf)

**MLA comparison.** The MLA Style Center gives `(Baron 194)` and `(194)` when the author is supplied in prose. In the checked CSL XML, the page branch joins author and locator with a space and omits the page label. The bibliography layout asks for a terminal period and a double-spaced hanging indent. Those rules match the examples at source level. A reported Hayagriva run omitted the final period. The report does not establish its cause or reproduce it on 0.9.1. The engine was not run for this design. (my call) Gate the release on exact styled-run fixtures for both Baron citations, the article's volume, issue, date and page range, the terminal period, Mantel's monograph, an edited chapter, a DOI article, a no-author web page, two titles by one author, and a cluster. Compare to MLA examples first and Pandoc second. Check title-case, italics, quotes, abbreviation, range, disambiguation and punctuation before and after the marker. Check the final Word and PDF too. [MLA in-text examples](https://style.mla.org/in-text-citations-overview/) · [MLA entries by format](https://style.mla.org/works-cited/citations-by-format/) · [MLA 9 CSL](https://github.com/citation-style-language/styles/blob/master/modern-language-association.csl) · [Hayagriva issue #215](https://github.com/typst/hayagriva/issues/215)


## Left to the builder

- Choose the smallest lossless interface for styled citation runs and pin the exact style and
  locale with their notices. Verify a two-page body plus Works Cited in Word and Typst.
- Use original fixture text that reproduces published [MLA Style Center](https://style.mla.org/formatting-papers/)
  and [Purdue OWL](https://owl.purdue.edu/owl/research_and_citation/mla_style/mla_formatting_and_style_guide/mla_sample_paper.html)
  sample layouts. Measure Typst's installed and update size before qualifying the Nix closure;
  the existing LibreOffice conversion remains the path for the existing non-MLA exports.

## Implementation order

| Task | Files | First test and expected result |
|---|---|---|
| 1. Qualify MLA rendering | `crates/grimoire-core/Cargo.toml`, `crates/grimoire-core/src/citations.rs`, `assets/csl/`, `crates/grimoire-core/src/pdf.rs`, `nixos/pkgs/app-tests.nix` | Compare styled citation runs and the bibliography with the fixtures above. Compile a small Typst proof and inspect real footnotes, page heads and Liberation Serif before packaging. |
| 2. Add Essay | `crates/grimoire-core/src/project.rs`, `crates/grimoire-core/src/project_files.rs`, `src/main.rs`, `src/app/overlays/shelf.rs`, `assets/help/01-getting-started.md`, `assets/help/26-your-projects.md` | Essay opens to three flat sections with a 3,000-word goal and MLA defaults. CLI and shelf use the same scaffold. The existing three shapes keep their defaults. |
| 3. Add portable project sources | `crates/grimoire-core/src/sources.rs`, `src/app/timeline.rs`, `os/scripts/wros-sync`, `os/tests/test_sync.py` | Import CSL-JSON and .bib losslessly in the supported fields, reject invalid inputs without overwriting files, and merge independent edits by key. Preview a restore that strands a citation. |
| 4. Add project source forms and the @ picker | `src/app.rs`, `src/main.rs`, `src/app/overlays/sources.rs`, `src/app/overlays/mod.rs`, `src/app/overlays/bar.rs`, `src/palette.rs`, `src/app/shell.rs`, `src/footnote_insert_ui_tests.rs`, `tests/driven.rs`, `assets/help/24-every-key.md`, `assets/help/27-the-menu.md` | At 80×24 and the three larger driven sizes, prove both panes, trigger boundaries, literal @@, paste, locator, suppression, stale target, Esc and one Undo. |
| 5. Ship preflight, Word and Typst PDF | `crates/grimoire-core/src/export.rs`, `crates/grimoire-core/src/export/docx_footnotes.rs`, `crates/grimoire-core/src/pdf.rs`, `src/app/overlays/readers.rs`, `os/tests/test_export_preflight_pty.py`, `nixos/pkgs/app-tests.nix`, `nixos/tests/mla-paper.nix`, `assets/help/18-compile.md` | CLI and desk report the same preflight errors. Inspect Word XML and rendered Word/PDF for heads, formatting, footnotes and Works Cited across pages. Failed PDF conversion leaves existing files intact. On the HP Stream run the offline workflow in DRM and Cage. |
