# rc24: essays, MLA 9 and Zotero (design)

> **Owner, 2026-11-22: Zotero is out.** Drop the Zotero sign-in, "Search Zotero…" and every other Zotero step from rc24. Essays, MLA 9 export, `sources.json`, manual entry, `.bib`/CSL-JSON import and `[@key, p. 12]` citations stay; the later [Brain, Library and Review design](2026-11-22-brain-library-review-design.md) writes citations into this format.

Owner decisions 2026-11-21, after rc22. Calls marked "(my call)" need review. rc23 retires the WrOS dashboard and moves productivity tools into small dialogs. This design builds on that desk. [Citation research](2026-11-21-citation-management-research.md) · [Zotero research](2026-11-21-zotero-client-research.md) · [rc22 design](2026-11-21-rc22-projects-youtube-keys-design.md)

## Why

The owner hands in MLA 9 papers and cites a few sources in each. They need a project that starts like an essay, a quick way to cite while writing, and a Word document or PDF they can turn in. They want formatted citations and real footnotes. They want to scan a QR and approve Zotero on a phone.

## Goals

1. Add **Essay** to **New project…** and to the command line.
2. Add a project source list, manual entry and file import. Insert a source and optional page at the cursor. Write and export using saved sources offline.
3. Produce an MLA 9 paper in Word and PDF. Format citations and Works Cited with Hayagriva. Keep public footnotes editable in Word and at the foot of the page in PDF.
4. Link a personal Zotero library using OAuth 1.0a. Search it on request and copy selected bibliographic records into the project. Keep access read-only. [Zotero OAuth](https://www.zotero.org/support/dev/web_api/v3/oauth)

## Non-goals

- Academic paper, poetry, memoir and the rest of #163. This release needs one form to prove the essay workflow. Academic paper would add different sections and submission defaults. (my call)
- A user template loader, support for more citation styles, DOI or ISBN lookup, and live Word citation fields.
- Offline Zotero library sync, attachment downloads and annotation writes. Those belong to [rc25](2026-11-21-rc25-zotero-library-pdfs-design.md).
- MLA layout in paperback or EPUB. The editable Markdown routes keep Markdown.

## Anti-goals

- A new dashboard, a global shortcut grabbed from the desk, or text inserted into the wrong pane.
- An essay with a fiction title page, chapter starts, ornamental breaks or an END line.
- A clean-looking paper with a missing citation, a lost footnote, or a substituted PDF font passed off as Times New Roman.
- A Zotero password typed into WrOS, a pasted API key, or an account secret in a project, Drive or a backup.

## Approaches considered

- **One shape or a template loader.** Add an Essay case to `Template` and reuse the current scaffold's safe file creation. (my call) The current shape models parts, chapter folders and scene files. Use the existing flat manuscript reader for Essay. An Essay needs flat sections and different starter files. Prove those with one built-in shape before fixing a TOML format for every kind of project. Keep the grouped picker from #163. [Current scaffold](../../../crates/grimoire-core/src/project.rs)
- **Hayagriva or Pandoc.** Try the checked 0.9.1 Hayagriva pin first with the CSL-JSON feature and an explicit MLA 9 style. Keep the CLI and bundled style archive out of the build. Use off-device Pandoc citeproc as an independent comparison. If the adapter or processor fails the fixtures, fix the narrow fault and rerun them. A separate local processor needs a size and licence decision before it ships. The MLA label waits for passing results. (my call) [Hayagriva manifest](https://github.com/typst/hayagriva/blob/v0.9.1/Cargo.toml) · [Hayagriva library](https://github.com/typst/hayagriva/tree/v0.9.1) · [Pandoc citations](https://pandoc.org/MANUAL.html#citations)
- **GitHub Pages and a typed verifier. Chosen.** (my call) GitHub Pages serves a static callback. The phone displays the verifier. The writer types it once on the laptop, which already holds the request-token secret. One recorded Zotero exchange used **20 hexadecimal characters**. The public docs give no fixed length or alphabet. Treat 20 as an observed length and measure the current length and alphabet on a disposable registered app. Show the code in short groups for reading. Accept a bounded opaque string and join the displayed groups and leave all other characters unchanged. Avoid a six-digit PIN design and any assumption about case. This costs one transcription and keeps Google and a relay out of the flow. [GitHub Pages](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages) · [Recorded exchange](https://stackoverflow.com/questions/53859439/oauth-1-0a-node-package-returning-401-for-zotero-api) · [Zotero OAuth](https://www.zotero.org/support/dev/web_api/v3/oauth) · [OAuth 1.0 verifier rules](https://www.rfc-editor.org/rfc/rfc5849#section-2.2)
- **Google Drive phone-link mailbox.** Reuse the QR look. Skip the transport. The current flow gets a Google limited-input grant, polls Drive for a short-lived ordinary file and removes it. Reusing it would add Google sign-in or a Google grant to Zotero onboarding, put approval data in a second service and add a second failure path. Zotero's documented third-party exchange uses its own request token, verifier and API key. [WrOS setup](../../GOOGLE_SIGN_IN_SETUP.md) · [Mailbox code](../../../os/scripts/wros-link.py) · [Zotero OAuth](https://www.zotero.org/support/dev/web_api/v3/oauth)
- **A small relay such as a Cloudflare Worker.** It could keep a short-lived callback result for the laptop to claim across networks. It would need an HTTPS endpoint, state, expiry, account ownership and abuse controls even on the free plan. Keep it as a fallback decision if real pairing shows transcription is too hard. [Cloudflare Worker pricing](https://developers.cloudflare.com/workers/platform/pricing/) · [Zotero callback contract](https://www.zotero.org/support/dev/web_api/v3/oauth)

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

These rows use the existing menus and groups. `Esc` opens the bar after dismissing a selection or pane where applicable. `F10` opens it directly. `Ctrl-K` opens the palette. Keep the current menu navigation, `↑/↓`, `←/→` and `Enter`. No new global key. The Edit menu gains one row beside **Insert footnote here**, within the rc22 limit of twelve. Keep `F11`, `Ctrl-O`, `Ctrl-F` and editor Undo as they are. Check the menu cap on the rc23 branch as well. [Menu](../../../src/app/overlays/bar.rs) · [Palette](../../../src/palette.rs) · [Settings routes](../../../src/app/shell.rs) · [Every key](../../../assets/help/24-every-key.md)

| Menu path | Ctrl-K entry |
|---|---|
| File › New project… | New project… (existing) |
| Edit › Insert citation here… | Insert citation at the cursor |
| Tools › Notes › Project sources… | Project sources |
| Tools › Notes › Add a source… | Add a source |
| Tools › Notes › Import sources… | Import sources |
| Settings › Writing › Manuscript › MLA 9 paper details… | MLA 9 paper details |
| Settings › Backups and accounts › Zotero › Connect Zotero… | Connect Zotero |
| File › Export… | Export for readers (Word, PDF, EPUB) (existing) |
| File › Compile to Markdown | Compile manuscript (Markdown) (existing) |

- **Writing › Manuscript** gains **MLA 9 paper details…**. The export dialog's **Paper details…** opens the same form. The form has **Name**, **Running surname**, **Instructor**, **Course**, **Date** and **Save**. The title comes from the project's title field. Offer the project author as a starting value and let the writer enter a compound surname as written. Show an example date such as `21 November 2026` and save the paper's date as entered. A draft with missing details can still be written. Export Review names each required field that is empty. (my call)
- **Backups and accounts › Zotero** shows **Connect Zotero…** when disconnected. Once connected it shows **Account · <name>**, **Access · Read-only**, **Clear Zotero search cache** and **Disconnect Zotero…**. The account and access rows show local status. **Connect Zotero** in the palette opens that page when already connected. Opening Settings makes no network request. (my call)
- In forms, `Tab` and `Shift-Tab` move between fields. `↑/↓` move through a focused list. `Enter` activates the selected row or the focused button. `Esc` backs out. Keep typed values during an import error or a failed Zotero query. A fresh dialog has its own search field and never interprets typed text as an outline key. Use a scrollable one-column layout at 80×24. In Export, keep `↑/↓` for rows and `Space` or `Enter` for format ticks. Use `←/→` on the new Layout row.

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
  [Search Zotero…]
  Pages [194       ]   [ ] Author named in text
  [Add another source]   [Insert]
  [@baron2013, p. 194]                    Esc back
~~~

In the citation picker, selecting a row gives it a locator field. Search project sources as the writer types. **Search Zotero…** opens a separate search field with an explicit **Search** button. `Enter` in that field submits. Results may have a **More results** row. Pick a result, check its bibliographic fields and confirm **Copy source and insert**. Open the picker only at a writable manuscript or note cursor. Bind the target to the original document, editor pane and revision. Recheck it before inserting, including after a remote response or a conflict. Preserve the cursor and selection on cancel. Insert one Undo step in the target pane. Undo removes the marker and leaves the reusable source in the list. Use the existing footnote insertion guards and tests as a model. [Footnote insertion tests](../../../src/footnote_insert_ui_tests.rs) · [Zotero search and pagination](https://www.zotero.org/support/dev/web_api/v3/basics)

### Sources and project files

- `sources.json` at the project root is a UTF-8 CSL-JSON array, initially `[]` for Essay and created on first source save for older projects. Store one stable case-sensitive `id` per record. Accept safe keys made from ASCII letters, digits, hyphens and underscores. Suggest author plus year for new entries and offer a suffix on collision. Preserve valid unused import keys. Preview any repaired key and each unmapped field before import. Match a clear duplicate for reuse with confirmation. Preserve valid fields and creator order. Guard deletion of cited records and lock the key on ordinary edits. Keep a malformed file intact and show the error. (my call) [CSL-JSON item schema](https://github.com/citation-style-language/schema/blob/master/schemas/input/csl-data.json)
- Manual entry supports monographs, edited chapters, journal articles and web pages first. Use a BibTeX/BibLaTeX parser for `.bib`, including macros, accents and protected capitals. Import `.bib` and CSL-JSON through a path field and preview. Show adds, duplicates and losses before saving a batch. Preflight accepts `[@key]`, `[@key, p. 12]`, `[@key, pp. 12–14; @other]` and `[-@key, p. 12]` for author named in the prose. For a name and title already supplied in prose with no locator, allow `[-@key]` only if the fixture proves it creates a bibliography entry without empty parentheses. Fail review on recognizable unsupported syntax. Leave ordinary prose, email, code, escaped brackets, links and images alone. (my call) [BibLaTeX documentation](https://ctan.org/pkg/biblatex) · [Pandoc citation syntax](https://pandoc.org/MANUAL.html#citation-syntax) · [MLA in-text examples](https://style.mla.org/in-text-citations-overview/)
- Store the template ID and export profile in `project.toml`. (my call) Use `[mla]` for heading and running-surname fields and `[citations]` for the style ID, qualified revision and `en-US` locale. Keep the style and locale in the release. Missing metadata on an old project keeps the existing export default. Choose the MLA profile in Export for any project. Turning it on does not rearrange existing files. Resolve old project settings via the rc22 helper. Write only the new names. [Project file helper](../../../crates/grimoire-core/src/project_files.rs) · [CSL styles](https://github.com/citation-style-language/styles) · [CSL locales](https://github.com/citation-style-language/locales)
- Copy bibliographic fields from the chosen personal item using the Zotero `csljson` response format. Validate and preview them. Keep the project record editable and usable after item deletion or disconnect. (my call) Save the account and item key to citation-ID lookup only in the local account cache, keyed by project identity. Avoid an origins file in the project. If the lookup is gone after a move or eviction, offer to reuse a matching project source without rewriting its fields. Save the source before inserting the marker. A failure after the source save can leave an unused source. [Zotero item formats](https://www.zotero.org/support/dev/web_api/v3/basics)
- Sort records by ID and write stable, readable JSON. Use an atomic replace and fingerprint check for local edits and imports. Use the project save and timeline hook. Root `sources.json` and `project.toml` travel with readable Drive, folder import, encrypted project backup and timeline snapshots. Show a **Project sources** file entry for timeline restore. A sources-only restore previews keys it would strand in the current text. The Drive helper already syncs visible files, while its text merge-base allowlist omits JSON. Add the exact root `sources.json` path to that bounded allowlist. Use base, desk and parked Drive copy to merge independent changes by ID. Offer a choice for two edits to one entry, delete versus edit and key collisions. Keep both copies when the base is missing or either side is invalid. Honor the sync and pending-file contract on save and resolve. Drive can deliver scenes and sources at different times. Recheck after sync and block formatted output with unresolved keys or pending conflicts in its inputs. [Sync helper](../../../os/scripts/wros-sync) · [Timeline](../../../crates/grimoire-core/src/timeline/mod.rs) · [Native backup](../../NATIVE_BACKUP.md)

### MLA output and proof

~~~text
Export
  Layout         [MLA 9 paper  ▾]   [Paper details…]
  Formats        [x] Word   [ ] PDF   [ ] Markdown
  [Review before export] p       [Export] x
~~~

- Add a named profile, **MLA 9 paper**, beside the existing manuscript layout. The profile defaults to whole-project US Letter. (my call) Make one Word section with 1-inch margins, Times New Roman 12 pt, double spacing and half-inch first-line indents in body paragraphs. Put the right-aligned header half an inch from the top, including page one. Use the entered surname, a space and a live Word PAGE field. At the top left of page one put name, instructor, course and date on four lines. Centre the plain project title above the text. Keep the numbered section files in reading order without fiction page breaks. Have `grimoire export --docx --pdf` honor the same saved profile and checks. Start **Works Cited** on a new page with the same running head, a plain centred heading, half-inch hanging indents and double spacing. (my call) Use **Works Cited** even for one entry, per the owner's label. The MLA example guide suggests the singular for one. With zero cited sources omit the page. [MLA paper layout](https://style.mla.org/app/uploads/sites/3/2020/12/Formatting-a-Research-Paper_v3_-The-MLA-Style-Center.pdf) · [MLA running heads](https://style.mla.org/adapt-mla-running-heads/)
- Use ordinary editable runs for styled citations and bibliography entries. Reuse the actual DOCX footnote machinery for scene-local `[^label]` definitions. Give MLA footnotes 12 pt and double spacing, as in the body, with superscript numbered references and continuous numbering. Citations inside referenced notes join the same Works Cited as body citations. Resolve markers over the exact included text and notes in reading order after excluding private notes and omitted material. Scope checks and source ranges must match the export. In Review (`p`), show file and line for bad keys, broken markers or notes and unresolved conflicts. Repeat checks before Export (`x`) publishes any companion file. Export leaves the Markdown input intact. Raw **Compile to Markdown** and Markdown export retain citation markers and warn about unresolved keys. If a reader format lacks citation rendering, stop that selection and offer Word, PDF or raw Markdown. (my call) [MLA footnote formatting](https://style.mla.org/note-formatting-in-microsoft-word/) · [Existing Word notes](../../../crates/grimoire-core/src/export/docx_footnotes.rs) · [Export review keys](../../../assets/help/18-compile.md)
- Convert the same Word file locally through `pdf::convert`. The paperback renderer in `export_print.rs` applies print trim, gutters and a separate conversion path. Use the manuscript path and name the outputs `Surname_Title_MLA.docx` and `.pdf` in `exports/`. (my call) Qualify a supported headless LibreOffice install on WrOS. The test closure has it, but the base system does not declare it. Provide an optional signed install path if needed. Check that the actual Times New Roman faces resolve and are licensed for local use and PDF embedding. Stage the PDF conversion and replace the final PDF only on success. A missing converter or font produces a PDF error and keeps the useful Word output. If an older PDF exists, identify it as the earlier export. The default Word styles name the face. For PDF acceptance inspect embedded fonts and render the pages. A CI render with substitute fonts can test structure only. Keep font files out of the public release unless there is a redistribution licence. [Current converter](../../../crates/grimoire-core/src/pdf.rs) · [Print path](../../../crates/grimoire-core/src/export_print.rs) · [Test closure](../../../nixos/pkgs/app-tests.nix) · [LibreOffice conversion](https://help.libreoffice.org/latest/en-US/text/shared/guide/convertfilters.html) · [Microsoft font permissions](https://learn.microsoft.com/en-us/typography/fonts/font-faq)

**MLA comparison.** The MLA Style Center gives `(Baron 194)` and `(194)` when the author is supplied in prose. In the checked CSL XML, the page branch joins author and locator with a space and omits the page label. The bibliography layout asks for a terminal period and a double-spaced hanging indent. Those rules match the examples at source level. A reported Hayagriva run omitted the final period. The report does not establish its cause or reproduce it on 0.9.1. The engine was not run for this design. (my call) Gate the release on exact styled-run fixtures for both Baron citations, the article's volume, issue, date and page range, the terminal period, Mantel's monograph, an edited chapter, a DOI article, a no-author web page, two titles by one author, and a cluster. Compare to MLA examples first and Pandoc second. Check title-case, italics, quotes, abbreviation, range, disambiguation and punctuation before and after the marker. Check the final Word and PDF too. [MLA in-text examples](https://style.mla.org/in-text-citations-overview/) · [MLA entries by format](https://style.mla.org/works-cited/citations-by-format/) · [MLA 9 CSL](https://github.com/citation-style-language/styles/blob/master/modern-language-association.csl) · [Hayagriva issue #215](https://github.com/typst/hayagriva/issues/215)

### Zotero login and network

~~~text
Laptop · Connect Zotero                   Phone · WrOS
  [QR for zotero.org approval]              Enter this code on the laptop.
  Approve on your phone.                    XXXX XXXX XXXX XXXX XXXX
  Code [                         ]          You can then close this tab.
  [Connect]          Esc cancel
~~~

- Register WrOS with Zotero. Use `https://j5cfk7qm49-web.github.io/WrOS/zotero/` as the proposed registered callback. From the laptop sign `POST /oauth/request` for that callback and require callback confirmation. Keep one pending temporary token and secret in memory. Show a QR for `/oauth/authorize` with the temporary token. Request personal library read, with notes and writes off and groups set to none. Treat form values as defaults the user can change. The phone signs into Zotero and approves there. The static callback displays only the verifier or a denial state, clears the query from the current history entry and uses no analytics, third-party assets or outbound requests. Treat the original callback URL as visible to the Pages host. The laptop signs `POST /oauth/access` with the pending token and secret and the verifier. Zotero checks that they belong together. Cancel, retry with a fresh token or expire the local session after ten minutes. (my call) Discard a late answer. Allow a corrected entry after a typo while the local session remains valid. Test different phone and laptop networks, a wrong token, denial, expiry, reuse and provider errors. [Zotero OAuth](https://www.zotero.org/support/dev/web_api/v3/oauth) · [OAuth exchange and temporary credentials](https://www.rfc-editor.org/rfc/rfc5849#section-2) · [Current Google visual flow](../../GOOGLE_SIGN_IN_SETUP.md)
- Verify account ID and actual access with `GET /keys/current` before storing. Require personal library read. Reject write, notes and private group grants and show how to retry with narrower access. Check the reported group-default issue using an account with a private group. If file access is bundled into read, verify it with Zotero and disclose it in the connection details. rc24 sends no file requests. Confirm Zotero's registration terms for a distributed native consumer before shipping. Use a separate release input `WROS_ZOTERO_CLIENT`, read by the build in the same way as `WROS_GOOGLE_CLIENTS`, with a proposed `/etc/wros/zotero-client.json`. The consumer secret never goes in Git or Pages. Keep it out of build and release logs. A shipped binary or system image exposes this shared consumer credential to someone who can inspect it. Keep rotation instructions. Builds without it show linking as unavailable and still provide manual sources, imports and exports. (my call) [Key verification](https://www.zotero.org/support/dev/web_api/v3/syncing#verify_key_access) · [Scope report](https://groups.google.com/g/zotero-dev/c/vA6FEMQ4XU8) · [Zotero terms](https://www.zotero.org/support/terms/terms_of_service) · [Distributed secret limit](https://www.rfc-editor.org/rfc/rfc5849#section-4.6) · [WrOS build input pattern](../../GOOGLE_SIGN_IN_SETUP.md)
- Put the per-writer key, account ID, display name and granted scope in `$XDG_DATA_HOME/grimoire/wros/zotero/credentials.json` (default `~/.local/share/grimoire/wros/zotero/credentials.json`). Use a 0700 directory, a 0600 regular file, atomic writes and symlink refusal. Use a bounded disposable local search cache of up to 4 MiB (my call). Keep both outside projects, Drive, timeline bundles, backups including settings, exports and logs. Use the v3 API key header over HTTPS and filter logs and errors. On disconnect cancel jobs, erase the local key and search cache, and offer Zotero's Security page to revoke the grant. Zotero's published server code resolves `current` to the header key and supports deleting it. If a live test confirms that for WrOS OAuth grants, use authenticated `DELETE /keys/current` online before erasing the local key and report the outcome. Revoke an overbroad grant by the same route when possible. [WrOS data path](../../../crates/grimoire-core/src/paths.rs) · [Zotero API authentication and deletion](https://www.zotero.org/support/dev/web_api/v3/basics) · [Zotero server key deletion](https://github.com/zotero/dataserver/blob/master/controllers/KeysController.php) · [Zotero Security](https://www.zotero.org/settings/security)
- Add the following to `docs/API_AND_NETWORK_USE.md` at build time. **Laptop → www.zotero.org** on Connect or retry, with OAuth consumer ID, signature, callback, nonce and timestamp, then temporary token and verifier at exchange. **Phone → www.zotero.org** on scan, with temporary token and consent defaults, plus its normal sign-in and approval. **Phone → j5cfk7qm49-web.github.io** on redirect, with temporary token and verifier or denial. **Laptop → api.zotero.org** for grant verification, explicit personal `/users/<id>/items/top` search with the submitted phrase, same-origin pagination and selected-item metadata. Document revocation separately if enabled. Include device permissions, what is cached, and what the callback host sees. Keep project titles, paths, drafts, locators and search-as-you-type local. Zotero operations need outbound HTTPS only. Respect `Backoff`, `Retry-After` and auth failures. [Network audit](../../API_AND_NETWORK_USE.md) · [OAuth endpoints](https://www.zotero.org/support/dev/web_api/v3/oauth) · [API limits and headers](https://www.zotero.org/support/dev/web_api/v3/basics)

## Left to the builder

- The exact width of form fields, and whether adding another contributor opens a row or a small edit box.
- The smallest lossless styled-run adapter and the precise fixture-qualified style revision. Review the bundled notices with the chosen dependency. [Hayagriva licence](https://github.com/typst/hayagriva/blob/v0.9.1/Cargo.toml) · [CSL style licence](https://github.com/citation-style-language/styles)
- How to provision the optional PDF converter in the signed build. Measure closure, update size and peak memory on the HP Stream before picking the delivery method. Verify a two-page body plus Works Cited in real Word and in the qualified PDF renderer.

## Outline plan

Effort for one builder, including tests. Allow about 15–22 working days. Registration, font access and live review can run alongside the local work.

| Order | Files | Proof | Effort |
|---|---|---|---|
| 1. Prove MLA, PDF and pairing | `crates/grimoire-core/Cargo.toml`, proposed `crates/grimoire-core/src/citations.rs`, `assets/csl/`, `crates/grimoire-core/src/pdf.rs`, proposed `pages/zotero/index.html` | Exact Hayagriva runs against the MLA fixtures. Converter and font probe on the installed system. Disposable Zotero grant, two networks, verifier shape, consent and expiry. | 3–4 days |
| 2. Add Essay and the source store | `project.rs`, `project_files.rs`, proposed `sources.rs` in `grimoire-core`, `src/main.rs`, `src/app/overlays/shelf.rs`, `assets/help/01-getting-started.md`, `assets/help/26-your-projects.md` | Proposed tests `essay_scaffold_and_cli` and `source_import_keys_and_conflicts`. Confirm grouped picker and unchanged defaults for all three existing shapes. | 2–3 days |
| 3. Add the desk and storage paths | `src/app.rs`, `src/app/overlays/mod.rs`, `bar.rs` and proposed `sources.rs` there, `src/palette.rs`, `src/app/shell.rs`, `os/scripts/wros-sync`, `src/app/timeline.rs`, `assets/help/24-every-key.md`, `assets/help/27-the-menu.md` | Proposed `essay_sources_at_four_sizes` in `src/footnote_insert_ui_tests.rs` or a sibling suite. Test both panes, undo, cancel, stale target, merge and menu cap. Extend VM `import-book` and `book-timeline` for sidecar restore, Drive conflict and no secret in backup restores. | 3–4 days |
| 4. Ship Word and PDF layout | `crates/grimoire-core/src/export.rs`, `export/docx_footnotes.rs`, `pdf.rs`, `src/app/overlays/readers.rs`, `assets/help/18-compile.md`, `nixos/pkgs/app-tests.nix` | Proposed `mla_word_pdf` in `os/tests/test_export_preflight_pty.py` and VM `mla-paper` in `nixos/tests/mla-paper.nix`. Unpack Word XML, verify citations and footnote links, render PDF and check font, paper dimensions, header on every page, heading, hanging indent and new page. Simulate missing font and failed conversion. | 3–5 days |
| 5. Connect and cite | Proposed `src/zotero.rs`, `pages/zotero/index.html`, `flake.nix`, `nixos/modules/base.nix`, `src/app/shell.rs`, `docs/API_AND_NETWORK_USE.md`, `docs/software-licenses.json` | Proposed VM `zotero-connect` in `nixos/tests/zotero-connect.nix` with a fake server and synthetic client. Assert read-only traffic, correct pagination, cancel, no secret in logs or backup, and offline export after disconnect. Add driven screen cases in `tests/driven.rs` at 80×24, 100×30, 120×40 and 160×50. On the HP Stream in DRM and Cage, pair live and submit a test paper to Word and PDF. | 4–6 days |

## Questions for the owner

1. Can you share a redacted paper that your instructor accepted and any grading sheet that sets layout exceptions?
2. Is typing a verifier of about 20 characters once per connection acceptable on the HP Stream?
3. Which licensed Times New Roman font source can we use on the HP Stream for PDF acceptance?

## Owner answers and a finding (2026-11-21, after this draft)

- **The login:** typing the verifier code shown on the GitHub Pages callback is fine.
- **MLA references:** use published MLA 9 sample papers (the MLA Style Center's and Purdue OWL's sample papers) as the fixtures the output must match. Their text isn't copied into the repository: build fixtures that reproduce their layout cases.
- **Times New Roman needs no licence.** A `.docx` only names the font, and the reader's computer draws it. WrOS can't bundle Times New Roman (Monotype/Microsoft) and doesn't need to: MLA 9 asks for a legible font such as Times New Roman. PDFs WrOS makes use **Liberation Serif** (SIL OFL), which has Times New Roman's metrics.
- **Finding: WrOS can't make PDFs today.**
  - `export_print.rs` and `pdf.rs` shell out to LibreOffice, which is not in the WrOS system closure (only in the app-test environment), and the system ships only DejaVu fonts. So the PDF goal needs its own renderer.
  - Recommended: **Typst** (in nixpkgs, 0.14.2 at the WrOS pin). It uses Hayagriva, ships an MLA style and embeds fonts. Add `pkgs.liberation_ttf` for the PDF.
  - The builder measures Typst's closure size and checks its MLA 9 output against the fixtures before committing to it. Adding LibreOffice (hundreds of MB on a 32 GB laptop) is the rejected alternative.
