# Catalogue search for citations (design)

Date: 2026-11-24. Branch `citation-search`, from `development` (8f85f42, rc28 plus the notice pin).

## Intent

When adding a source, the owner wants to type whatever they have in hand and pick the right
match from a catalogue. That could be an author, a title, an ISBN, a DOI or a web address.
WrOS then fills in the citation details. Today every field in **Add a source** is typed by hand,
and the Library's DOI/ISBN lookup needs the identifier first.

The search should also work from `@` while writing, so that citing a new source never leaves the
sentence.

The owner cites mostly books, but also journal articles and web pages. It must not be
books-only. Online search must be something they can turn off.

## Goals

- **One search box, many kinds of input.** It recognises each by its shape:
  - an ISBN-10 or ISBN-13 with a valid check digit: exact edition lookup (Open Library);
  - a DOI (`10.NNNN/…`, `doi:…`, `https://doi.org/…`): exact work lookup (Crossref);
  - an `http(s)://` address other than doi.org: read the page's own metadata;
  - anything else is free text across title, author, publisher and year, in Open Library
    (books) and Crossref (articles, chapters and other scholarly works), merged into one list.
- **The list narrows as you type.**
  - Books show one row per book, with the edition that best matches the words typed so far.
    Typing a publisher, year or translator brings that edition forward. Open Library's
    `search.json` with `fields=…,editions,editions.*` already returns the best-matching edition
    per work; checked 2026-11-24: "crime and punishment penguin" returned Penguin Books 1998,
    ISBN 9780140621808.
  - **→ on a book row** opens all of that book's editions in a scrollable list. English editions
    come first, and more load on scroll. Pick one to cite it.
- **Both places.**
  - The `@` picker (`Page::Pick`): local results first (this project's sources, then the
    Library, as in rc28), then a **Catalogue** section.
  - The top of **Add a source** (`Page::Form`): a search line, where a pick fills every field
    for the writer to check before saving.
  - My call: the Brain's `@` picker (`Page::BrainPick`) gets the same Catalogue section.
- **A pick is kept locally.** It becomes a Library edition, with the same provenance an rc27
  Library pick carries, and then goes into the project's `sources.json` and the citation, exactly
  like a Library pick today. Next time it appears in the local results without going online.
  - If the laptop has no Library store yet (`library_store_if_present()` is `None`), the pick
    goes into `sources.json` only. My call; a builder may instead create the Library store on
    first pick if that is what `@` already does.
- **Off switch.** A Settings row, "Search catalogues online from @ and Add a source", defaults to
  on.
  - The Catalogue header always says whether it is on.
  - With it off, `@` and Add a source are local-only, as in rc28. The Library desk's explicit
    `d` and `l` lookups keep working, because they are a deliberate Enter on one identifier.
  - Place the row in the Settings group that holds writing/Library preferences, following the
    five-group layout.

## Non-goals

- No accounts, API keys or sign-in (the owner dropped Zotero over sign-in, 2026-11-22).
- No offline catalogue copy (Open Library's dumps are tens of GB; the laptop has 32 GB).
- No PDF or full-text fetching, and no library databases behind logins (JSTOR, EBSCO).
- No new citation styles; MLA 9 output stays as rc24 makes it.
- No change to how citations are written into the text (`[@key, p. N]`) or to `sources.json`'s
  format.

## Anti-goals

These would make it a failure even if it technically works:

- **Typing lags or a keystroke is lost** while a search runs. Network work never runs on the UI
  thread, and a slow catalogue never delays local results.
- **A wrong edition is cited silently.** A free-text pick always shows the publisher and year in
  the row, and Add a source shows every field before saving.
- **Leaking writing.** Only the search line is sent: never project text, notes, the sentence
  around `@`, account identifiers or email. Sending starts only after 3 typed characters and a
  pause (see Constraints).
- **A wall of errors offline.** With no network, the Catalogue section shows one quiet line
  ("Offline: showing local sources") and nothing else.

## Constraints

- **Network rules** (as in `lookup.rs` today, applied to every new request):
  - HTTPS only, the generic User-Agent `WrOS Library`, a 10 s timeout and a 1 MiB reply cap.
  - No redirects for the catalogue APIs. Web pages may follow up to 3 redirects, HTTPS to HTTPS
    only, and read at most 2 MiB, stopping after `</head>`.
- **Rate.**
  - A request goes out only after a 500 ms pause with ≥3 characters in the line.
  - At most one request is in flight per catalogue, and at least 1 s passes between requests
    to the same catalogue.
  - When the line changes, the reply to the old line is dropped, not shown.
  - Keep a small in-memory cache of recent replies (like the 16-entry `recent_lookups`), so
    deleting a letter and retyping doesn't refetch.
- **Crossref results are noisy.** A test query ranked a review first. Keep only these types:
  `journal-article`, `book`, `book-chapter`, `monograph`, `edited-book`, `reference-entry`,
  `proceedings-article`, `report`, `dissertation`, `posted-content`.
  - Drop `peer-review`, `component` and similar.
  - Show at most about 8 Crossref rows under the book rows (books first, since the owner cites
    mostly books).
- **Open Library search returns works.**
  - Edition rows need the edition's own title, publisher, date, ISBN and, where given,
    `edition_name`, `publish_places` and `by_statement` or contributors. The work's title can
    be in another language (the test returned the Russian title), so use the edition's title.
  - The editions list uses `/works/OLxxxW/editions.json` (paged, 1,178 editions for *Crime and
    Punishment*). English (`/languages/eng`) sorts first within each loaded page. A builder may
    find a better server-side filter (for example `search.json` `lang=` or an `editions.limit`
    field); try it in a short spike.
- **Web pages.** Read, in order of trust:
  1. Highwire/Google Scholar `citation_*` meta;
  2. schema.org JSON-LD (`Article`, `NewsArticle`, `BlogPosting`, `WebPage`: `headline`/`name`,
     `author`, `datePublished`, `publisher`);
  3. Dublin Core `DC.*`;
  4. Open Graph (`og:title`, `og:site_name`, `article:published_time`);
  5. `<title>`.
  Produce a CSL `webpage` record with `URL` and `accessed` set to today (MLA 9 uses the access
  date when no publication date is given).
- **Record shape.** Results map to the CSL-JSON keys `sources.json` already uses (`type`,
  `title`, `author`, `editor`, `translator`, `issued`, `publisher`, `publisher-place`,
  `container-title`, `volume`, `issue`, `page`, `edition`, `DOI`, `ISBN`, `URL`, `accessed`).
  - Reuse `crossref()` and `open_library()` in `src/app/brain/library/lookup.rs`.
  - Fill `translator` when the catalogue gives one: MLA 9 needs it for translated books.
  - Add a Translator field to Add a source's form, because a filled-in translator must be
    visible and editable there.
- **The Library's acceptance rules still hold.** A catalogue pick enters the Library through the
  same path the Library desk's import review uses (`accept_import`), so provenance and
  fingerprints match what `insert_citation` checks.
- **Docs.** Update `docs/API_AND_NETWORK_USE.md`'s "Library lookups" section: what is sent to
  whom, when, the off switch, and the web-page fetch (which contacts whatever site the address
  names).
- **Tests.**
  - No test touches the real network. Use the same kind of seam the lookup tests use (fixture
    replies), including an offline case, a stale-reply case (old line's reply dropped) and a
    slow-reply case (typing stays responsive).
  - The existing app/Rust suites and `tools/nix-gates.sh` must pass.

## Approaches considered

1. **Extend the in-app lookup (chosen).** It grows `lookup.rs` into a catalogue module with
   search requests beside the exact ones, run on background threads with results polled each
   tick into the open dialog. It reuses the code, network rules and tests that already exist.
2. **A separate helper program** (like `wros-sync`). It isolates the network code, but
   duplicates the lookup and starts a process per pause, for no gain over 1.
3. **An offline catalogue copy.** It needs no network and sends nothing, but is far too large
   for the laptop.

The owner left the how to me; I chose 1.

## Decided with the owner (2026-11-24)

- Search runs live from `@`, with a setting to turn it off.
- It covers books, journal articles and web pages, not only books.
- One box takes an ISBN, a DOI, an address, an author or a title.
- The list narrows as more is typed; a book row opens all its editions, which can be scrolled.
- The mock-up below "looks good for now".

```
@crime and punishment penguin
 This project
   dostoevsky-crime   Dostoevsky, Crime and Punishment · Penguin 2003
 Catalogue                                       online search on · F2 off
 ▸ Crime and Punishment · Dostoevsky · Penguin Books 1998 · ISBN 0140621806   1178 editions →
   Crime and Punishment [adaptation] · Taylor · Pearson 2006                   2 editions →
   "Crime and Punishment" in Dostoevsky Studies · article · 1993 · DOI
```

The `F2 off` hint in the mock-up is illustrative. The real off switch is the Settings row; any
in-picker key is the builder's call, and must not clash with existing picker keys.

## Left to the builder

- Module layout, thread and channel plumbing, and where polling hooks into the tick.
- Exact ranking and merging between the two catalogues beyond "books first, then up to 8
  articles", and how many rows load per page.
- Citation-key suggestion for catalogue picks (reuse the existing suggestion rules).
- Row wording and truncation at 80 columns.
- How the editions list shows loading and the end of the list.
