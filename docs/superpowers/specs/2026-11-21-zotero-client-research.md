# A Zotero client for WrOS

21 November 2026

## Verdict

Build **Connect Zotero**, a small source picker, and offline MLA 9 export. Start with read-only access to the writer’s personal library. A writer should be able to scan a QR code, sign in on zotero.org, choose a reference, add a page number, and carry on writing. Keep the cited record in the project so it travels with the manuscript. Include manual entry and CSL-JSON import so the same workflow works on an offline laptop. [Zotero Web API][api] · [OAuth][oauth] · [Earlier WrOS report][earlier]

Keep a full client as a possible second product milestone. A full metadata client is feasible in Rust using the published sync protocol. It becomes worth building if the owner wants to manage a library on WrOS between writing sessions. File sync and a PDF reader each need a separate decision. For a few citations per paper, the first release needs neither. [Zotero sync protocol][sync] · [Data and file sync][data-sync]

## Login options

| Route | What the evidence shows | Decision for WrOS |
| --- | --- | --- |
| Username and password in the laptop app | Zotero’s older desktop flow sent a JSON username and password in `POST https://api.zotero.org/keys`. The client requested an automatic key with personal read, notes, write and file access, plus group read and write. It saved the resulting key in its login manager. Staff confirmed the password was discarded and later said older app versions could no longer sign in for security reasons. [Desktop 9.0.6 API source][desktop-9-api] · [Staff on password storage][staff-storage] · [Staff on older logins][staff-old] | Skip the password form. The public docs direct third parties to OAuth or a dedicated key. I found no public authorization or assurance of tolerance for third-party password exchange. The terms restrict unauthorized uses and access to non-public service areas. They give no endpoint-specific determination. The source shows how an official client worked. It gives no assurance that WrOS may use the same route. [API authentication][api] · [Terms, section 6][terms] |
| New first-party web login | Zotero 9’s changelog records the move to browser login. Desktop creates a login session at `/keys/sessions`, opens its login URL, then polls or listens for completion. The completion gives the app its API key and account identity. The official mobile source uses the same session family. Zotero’s web sign-in supports two-step verification and passkeys. [9.0 changelog][desktop-9-news] · [Web login change][desktop-web-login] · [Desktop session code][desktop-runner] · [iOS login][ios-login] · [Android auth API][android-login] · [Account verification][mfa] | Ask Zotero whether WrOS can use a supported version with read-only scope. Phone approval looks plausible because the client checks the server session. I could confirm neither cross-device completion nor public third-party access, scoped grants, or a stable contract for this endpoint. An official, scoped device flow would remove the need for a WrOS callback service. |
| Documented OAuth 1.0a with a phone browser | Register a WrOS app. The OAuth exchange uses a client key and secret and temporary credentials. The writer authorizes at `www.zotero.org/oauth/authorize`. The registered callback receives the temporary token and verifier. The exchange returns the writer’s user ID and a lasting API key. Zotero staff confirmed that an invalid verifier can cause exchange failure. [OAuth v3 and sample][oauth] · [Staff on verifiers][staff-verifier] | **Recommended release path.** Show a QR code or short WrOS URL. The phone opens Zotero’s own sign-in and consent screens. Send the callback to an HTTPS service operated by WrOS. The laptop collects the result over outbound HTTPS. A callback to `localhost` would arrive on the phone. The QR code solves the browser handoff. The HTTPS callback solves the return trip. |
| Native PIN, manual verifier, or device code | OAuth 1.0a defines an `oob` callback value. Zotero’s public OAuth docs show a callback URL and describe only OAuth 1.0a. I found no documented Zotero PIN display, `oob` promise, or OAuth 2 device-code grant. The R package `zoteror` documents pasting the verifier from the browser’s return URL as a PIN. That is evidence of a manual handoff design. I did not verify that it still completes with current Zotero. [OAuth specification][oauth-rfc] · [Zotero OAuth][oauth] · [API authentication][api] · [zoteror instructions][zoteror] | A WrOS-owned callback could display its own short confirmation code if needed. Test a code-only transfer if the owner wants to minimize relay state. Avoid asking the writer to copy a full callback URL. |
| Dedicated key, public reads, local access, and export | Zotero documents user-created keys and unauthenticated reads of public libraries. Its local API reads from a running desktop app on the same device. Zotero 10+ also supports locally approved writes with a separate key. CSL-JSON and BibTeX are file handoffs. [API authentication][api] · [Local API][local-api] · [Export formats][formats] | Keep export as the normal offline fallback. A manual key can help during prototype testing. Public reads and the local API cannot sign a writer into their private Web library from WrOS. |

### What the writer sees

In **Settings › Backups and accounts › Zotero**, choose **Connect Zotero**. Scan the QR or type the short link in a phone browser. Sign in and approve on Zotero. The laptop displays the account name and offers **Browse library**. Use the phone-link screen design already present in WrOS. The existing Google flow uses its own Google clients and Drive mailbox. Use the same visual pattern and build a separate OAuth transport. [Existing phone-link setup](../../GOOGLE_SIGN_IN_SETUP.md) · [Phone-link offer and QR](../../../src/backup_service.rs) · [Settings rendering](../../../src/app/shell.rs) · [Zotero OAuth][oauth]

For the first spike, keep the OAuth application secret on the WrOS callback service. A static Pages page cannot do that job. The laptop starts a short-lived session and retains a high-entropy claim secret. The QR carries a separate one-use browser link. Bind the callback to the expected temporary token, consume the result once, and expire abandoned sessions. The service handles the key transiently and never puts it in a URL or logs. The phone can show “Return to WrOS.” Test cancel, denial, expiry and races with late approval. Run the laptop and phone on different networks in the acceptance test. The owner would need to operate this small service and disclose its role. [OAuth registration and callback][oauth] · [Staff discussion on application secrets][staff-secret] · [WrOS Google setup](../../GOOGLE_SIGN_IN_SETUP.md)

Request personal library read access. Leave write, notes and groups off for release one. Zotero documents consent-form defaults. Check the resulting `GET /keys/current` permissions before storing the key, and refuse grants outside this release’s supported scope. Test an account with private groups. A developer reported that `all_groups=none` still produced group read access. I could read the subject and preview in the developer-list index. The full thread was unavailable, so the cause and any fix remain unconfirmed. Ask Zotero about that case and any file-access default. The WrOS client will make personal item requests only in release one. [OAuth scope parameters][oauth] · [Key access check][sync] · [Scope report][scope-report] · [Developer-list index][dev-list]

### Storing the key and signing out

Use `$XDG_DATA_HOME/grimoire/wros/zotero/credentials.json`, defaulting to `~/.local/share/grimoire/wros/zotero/credentials.json`. Give the writer’s directory mode `0700` and the file `0600`. Use atomic writes and reject symlinks. Store only the grant and account ID needed by WrOS. Send the key in `Zotero-API-Key` over HTTPS. Keep it out of `sources.json`, Settings exports, project history, Drive, backup payloads, command arguments and logs. Test the **Restore with settings** path as well as routine backup. File permissions limit access by other local users. A stolen unlocked machine can still expose the grant. [WrOS data paths](../../../crates/grimoire-core/src/paths.rs) · [Backup scope](../../NATIVE_BACKUP.md) · [API authentication][api] · [Staff on local credentials][staff-storage]

On sign-out, stop work and invalidate in-flight login and refresh. If online, revoke the active grant and erase the local key. Zotero desktop issues authenticated `DELETE /keys/current`. I found a public contract for `GET /keys/current` and could not confirm a public contract or live `DELETE` behavior for a third-party OAuth grant. Confirm it with Zotero and test on a disposable grant before promising one-click remote revocation. The documented account route is to remove the WrOS key in [Zotero Security](https://www.zotero.org/settings/security). If offline or if remote revocation cannot be verified, erase the laptop key, report that server revocation is pending on the writer, and show that page. Clear the downloaded library cache on release-one sign-out. With future write support, prompt to preserve or settle unsent edits before clearing. Project citations remain usable. [Desktop revocation code][desktop-9-api] · [OAuth key lifetime][oauth] · [Staff on removing app keys][staff-security]

Changing a Zotero password does not end desktop sync. The staff guidance is to revoke that computer’s key separately. The OAuth documentation likewise describes its keys as valid until revoked. Treat the WrOS grant the same way. I did not run a password-change test against an OAuth grant. Pause sync and offer reconnection when access fails. Avoid repeated failed requests. [Staff on password changes][staff-password] · [OAuth key lifetime][oauth] · [Staff on repeated auth failures][staff-failures]

## Clients to learn from

Dates below are the latest release or source activity verified in this pass. Candidate code was inspected or read about in its upstream documentation. No client was built. Use the published API as the contract, independent Rust for sync, and first-party clients as behavior references. [Zotero API and client list][api-index] · [Zotero licensing][z-license] · [WrOS licence](../../../LICENSE)

| Client | Licence, language, maintenance | Capability and reuse |
| --- | --- | --- |
| Pyzotero | Python, Blue Oak Model License 1.0.0 at the pinned 1.11.0. Published March 2026. Upstream activity in November 2026. [Pinned release][pyz-pin] · [Upstream activity][pyz-commit] | Web reads and writes, pagination and local API reads. It expects a key for private Web access and supplies API building blocks. A local database, outbox and conflict UI are WrOS work. Best helper option if Rust transport work overruns. Use bounded JSON over private stdin/stdout, like [`wros-ytm`](../../../os/scripts/wros-ytm). Use it first as a comparison tool. [Pyzotero API docs][pyz-doc] |
| `zotcli` | Python, MIT. PyPI 0.3.0, May 2026. [Package and history][zotcli] | Reads the installed Zotero SQLite database. Its docs describe connector writes and narrow direct database edits. It needs an existing desktop library and does not implement independent Web sync. Learn from its command structure. Keep its direct database access out of WrOS. [Package architecture][zotcli] · [Zotero database guidance][direct-db] |
| `zotero-cli` (riccardo-enr) | Rust. April 2026 commit. I found no reuse licence in its repository listing or Cargo manifest. [Repository][rust-cli] · [Manifest][rust-manifest] · [Commit][rust-commit] | Local/API browse, notes, collections and advertised add commands. Web configuration uses an API key. No persistent two-way sync was evident from the documented commands. Reference for compact JSON and command design. |
| `zotero-cli` (jbaiter) | Python, MIT. Latest checked commit August 2017. [Repository and licence][old-cli] · [Commit][old-cli-commit] | Search, open attachments, and create or edit notes through an external editor. I found no complete offline sync contract. Useful as a small note-editing example. |
| `zui` | Go, using Bubble Tea. March 2026 commit. No licence found in the checked tree. [Repository][zui] · [Commit][zui-commit] | A real TUI. Prompts for key and user ID, uses a system keyring, caches library reads and supports note edits. Its refresh code fetches and upserts collections. I found no evidence of full offline two-way sync. Use its browse and filter patterns. [Refresh source][zui-sync] |
| Refero | Python CLI and optional `fzf` picker, MIT. October 2026 commit. [Features and licence][refero] · [Commit][refero-commit] | Uses desktop for local reads or Web ID and key for remote CRUD. Offers reviewed writes and version checks. No complete replicating library is documented. Useful model for previewing changes and checking writes. |
| Zotero for Emacs | Emacs Lisp, GPL-3.0 per the repository. January 2024 commit in the mirror. [Repository][emacs] · [Commit][emacs-commit] | Offers OAuth authorization or a manual key, a library browser and a persistent cache. Documents cache synchronization and editing commands. I did not verify complete offline write and conflict handling. A useful comparison for the OAuth and picker workflow. |
| `retorquere/zotero-sync` | TypeScript, AGPL-3.0. Version 1.1.3, August 2025. Listed in Zotero’s API docs. [Manifest][ret-sync-manifest] · [Repository][ret-sync] · [API client list][api-index] | One-way library sync with an API key and a JSON store example. Read it for client-side version tracking. The runtime, licence and lack of two-way sync make it a poor shipping dependency. |
| Zotero desktop | JavaScript on Gecko. AGPLv3 by default, with per-file notices. Actively released. [Source and licence][z-source] · [Changelog][z-news] | Full data and file sync. Read `syncAPIClient.js`, `syncRunner.js`, `syncEngine.js` and tests for edge cases. Reimplement the public protocol in Rust. Porting the desktop sync engine would pull in much more of its application model. [Sync implementation][desktop-sync-code] · [Sync guide][data-sync] |
| Zotero for iOS and Android | Public source. Swift and Kotlin respectively. Zotero states AGPLv3 by default. I did not verify complete app-by-app dependency notices. The iOS build calls out optional licensed PSPDFKit. Both repositories had November 2026 commits. [Licensing][z-license] · [iOS source][ios-src] · [Android source][android-src] · [iOS activity][ios-commit] · [Android activity][android-commit] | Official apps sync personal and group data, edit library items and notes, and read and annotate PDFs. Current source uses first-party browser login sessions. Learn from mobile sign-in, restricted groups, and on-demand files. The app code is tied to mobile tooling. Review the actual notices before using any code. [iOS features][ios] · [Android features][android] · [iOS login][ios-login] · [Android auth API][android-login] |

Third-party login examples vary. Zoo for Zotero uses OAuth with an app callback in an Android WebView. PaperShip’s old OAuth flow broke for users, and Zotero staff described the app as unmaintained. ZotFile was a desktop attachment-management plugin with a tablet-folder workflow. The archived Obsidian Zotero Integration depends on Better BibTeX and desktop. ZotFlow uses a writer-created Web API key and its own worker and store. `papis-zotero` can import Zotero BibTeX exports. The official browser Connector also implements OAuth for its web path. Its callback runs through the extension. These projects show several ways to hand off a reference or finish an OAuth flow. [Zoo source][zoo] · [Staff on PaperShip][papership] · [ZotFile][zotfile] · [Obsidian integration][obsidian] · [ZotFlow][zotflow] · [papis-zotero][papis-zotero] · [Connector callback][connector-callback]

The owner verified the checkout’s nixpkgs pins on 21 November. They include `pyzotero` 1.11.0, `hayagriva` 0.9.1, `bibiman` 0.15.0, `papis` 0.15.0, `pubs` 0.9.0, desktop `zotero` 9.0.6 and `bibtexparser` 1.4.4. They exclude `papis-zotero` and `cobib`. Use those supplied values for WrOS estimates. The earlier report compares the file-based tools. In particular, bibiman is a useful model for keyboard-driven selection in Rust. [Earlier tool comparison][earlier] · [Pyzotero 1.11.0][pyz-pin] · [Hayagriva 0.9.1 manifest][hay]

## What a full client would do

### Library and sync

Keep one local library per account and personal or group ID. Show a library selector, collection tree, tags, search, item details, child notes and trash. Allow editing fields and creators, tagging, moving items between collections, and adding items by hand. Preserve unknown fields and item types. Use Zotero’s item templates and field metadata. Expose a clear read-only state where the key or group role lacks permission. Store saved-search definitions if syncing the complete metadata set. Zotero’s Web API does not execute saved searches. Implement their evaluation only after testing the rules. [Web API endpoints and item templates][api] · [Write requests][writes] · [Group permissions][groups] · [Saved-search API difference][local-api]

1. **Verify access.** Validate `/keys/current`. Check group metadata versions at `/users/<id>/groups?format=versions`. Fetch changed group definitions. Track group membership and edit rights apart from library data. Stop serving an inaccessible group from the ordinary cache. Protect any unsent work and offer recovery before removing it. [Sync steps 1 and 2][sync]
2. **Pull.** Save a library checkpoint and each object’s server version. Start at `since=0`. Fetch versions for collections, searches, top-level items, then all items including trashed ones. Pull changed objects in chunks of up to 50, with parents before children. Include `/deleted?since=…`. `Last-Modified-Version` ties the reads together. If the library changes during the pass, restart with increasing delay. Use `If-Modified-Since-Version` for a cheap idle check. `304` can still leave previously failed rows to retry. Commit the checkpoint only with the saved changes or durable retry markers. [Full-library sync algorithm][sync]
3. **Push.** Store an offline edit, its base and its outbox entry in one transaction. Try pending full-library writes with `If-Unmodified-Since-Version` at the last good library version. Use the object version for single-object or per-item writes. New locally keyed objects start at version `0`. Handle batch successes and failures separately. On `412`, pull, settle differences, then retry with the current version. Treat `428` as a client error. Reconcile keys and versions after ambiguous timeouts before retrying creates. Use the API’s write token for applicable unversioned requests. Keep one job per library. [Sync write order and versions][sync] · [Write requests and tokens][writes]
4. **Resolve.** Keep base, local and remote values for dirty records. Accept independent field changes after validation. Ask on overlapping edits, incompatible creator order, and edit versus deletion. Show the last synced version and unsent changes. Trash and restore are versioned item edits using the deletion flag. Treat server tombstones and permanent deletes separately. Keep any local permanent delete in the outbox until confirmed. Defer an empty-trash command. [Conflicts and deletions][sync] · [Item write API][writes] · [Desktop collections and trash][tags]
5. **Recover.** Respect `Backoff` even on success and `Retry-After` on throttling or downtime. Slow down on `429` or locked libraries, retry transient errors with bounded jitter, and pause on auth failure. Make a pull resumable after sleep, restart, or connection loss. Keep browsing and edits available offline with a last-sync timestamp. Consider optional WebSocket notifications later. [Backoff and server status][api] · [Rate-limit guidance from staff][staff-rates] · [Streaming API][stream]

### Adding items, notes, and files

- **Identifiers.** Add explicit lookup to the manual form. Use Crossref, then offer DataCite after a definitive DOI miss. Use Open Library for an ISBN edition and arXiv’s API for an arXiv ID. Show a preview and let the writer fix fields or save the ID for later. The Zotero Web API provides item templates and item writes. I found no documented general hosted translator endpoint for WrOS to depend on. Zotero’s separate AGPL translation-server can translate identifiers and webpages via `/search` and `/web`. Use it as an off-device comparison. Revisit hosting only if arbitrary webpage capture warrants a Node runtime or another service. [Crossref][crossref] · [DataCite][datacite] · [Open Library][openlib] · [arXiv][arxiv] · [Zotero adding items][add-items] · [Translation server][translation] · [API index][api-index]
- **Notes and annotations.** Preserve original note HTML and render a sanitized text or styled view. Block remote images and active content. Support basic paragraph, emphasis, list and link editing first. Round-trip existing rich notes before editing them. Zotero notes can carry structured citation data in HTML attributes. Preserve markup the editor cannot safely change and show it read-only. PDF annotations are separate synced records tied to an attachment and position. Start by showing quote, comment, author and page label. Inline highlights, geometry, ink, images, and annotation creation need a viewer and cross-client fixtures. Keep library notes distinct from a project’s markdown notes. [Zotero notes][notes] · [Note schema in source][note-code] · [Annotation storage][annotations] · [Annotation fields in source][annotation-code]
- **Attachments.** The metadata client can list child files with filename and type. A later release could fetch one stored attachment at a time, using the library’s `/items/<key>/file` route, with bounded redirects, validation and an evictable cache. File sync is separate from data sync. Zotero Storage offers 300 MB free and paid 2 GB, 6 GB and unlimited plans. Group files count against the owner’s quota. WebDAV can sync personal attachments through a separately configured server and credentials. Group files need Zotero Storage. Linked local files remain the writer’s files on the other device. Defer upload and WebDAV. The published file API uses an authorization step, transfer and registration. A later client would need quota and interrupted-upload handling. [Desktop download code][attachment-download] · [Data and file sync][data-sync] · [Plans][storage] · [Groups and quota][groups] · [Attachments][attachments] · [File API][file-api] · [WebDAV providers][webdav]
- **Reading PDFs.** The lightest option is a text-only preview with Poppler’s `pdftotext`. Scanned pages can produce empty text. A later graphical preview could rasterize one page with `pdftoppm`, then draw it through WrOS’s DRM or Cage path. A 1366×768 RGBA page takes about 4 MiB before decoder and renderer overhead. Grimoire’s page is currently drawn as terminal cells. Pixel display would need a renderer and real zoom and scrolling tests. Try text first. Keep the browserless citation release usable without downloads or rendering. [Poppler text tool][pdftotext] · [Poppler page tool][pdftoppm] · [WrOS graphical session](../../GRAPHICAL_SESSION.md) · [DRM renderer](../../../crates/wros-graphics/src/kms.rs)

### Addendum: terminal PDF readers built with ratatui (added after this report)

The owner pointed to [pdf-tui](https://github.com/WindustH/pdf-tui), read on 21 November 2026 together with [tdf](https://github.com/itsjunetime/tdf):

- **pdf-tui** is MIT and written in Rust with ratatui 0.30.2, the same version as Grimoire.
  - It renders pages with pdfium (default), poppler or mupdf, and shows them through the Kitty, Sixel or iTerm2 picture protocols, with Chafa character art as a fallback.
  - It has search, bookmarks with page previews, and copying a region as text or PNG.
  - It calls out to `pdfinfo`/`pdftotext`, `chafa`, `exiftool` and `pdftk`.
  - It is small: 22 stars, last commit 31 October 2026.
  - It is the one to learn from or borrow from.
- **tdf** is AGPL-3.0-only, uses mupdf and the Kitty protocol, and its README refuses AI-assisted contributions. Leave its code alone.
- **For WrOS:**
  - Grimoire's default DRM session isn't a terminal emulator, so neither tool's picture protocols apply there. WrOS's own renderer already draws pictures (the boot logo), so a page image could be drawn natively, sharper than any terminal protocol.
  - The Foot-under-Cage session speaks Sixel, so pdf-tui's approach would work there without changes.
  - Phase 4's text preview first, then a native page image, still stands.

## Writing and export

Selecting a reference copies **bibliographic fields only** into the project’s root `sources.json`, a CSL-JSON array with stable citation IDs. Zotero Web API v3 offers `csljson` in its item response formats. For the first release, request CSL-JSON for the selected item and validate it. If native conversion is needed for a missed field, test that mapping from Zotero editable JSON. Keep optional origin metadata keyed by project citation ID. Store only the library kind/ID, item key, accepted version and any comparison fingerprint. An optional proposed `source-origins.json` can travel with the project so refresh works after a move. Tell the writer that readable Drive would copy those IDs in plaintext and allow disconnecting the origin. Omit origins from reader handoffs. A fresh login to the matching library can fetch an updated record. Show a diff and preserve the stable citation key, edits and locators. A deleted or inaccessible source keeps its last accepted snapshot. [API item formats][api] · [CSL-JSON schema][csl-json] · [Staff on export fidelity][export-fidelity] · [Earlier WrOS data plan][earlier]

Insert `[@baron2013, p. 194]` at the saved cursor in one Undo step. Support the suppressed-author marker `[-@baron2013, p. 194]` for sentences that already name the author. Use the project sidecar for exports. Resolve actual Markdown ranges and exclude code, private notes and omitted scenes. Warn on missing or disputed sources and stop styled output before publishing incomplete files. Follow the earlier report’s atomic-save, Drive conflict, timeline and legacy-project plan. Baseline release targets are Word manuscript, its optional PDF companion, and a portable editable Markdown handoff. Paperback and EPUB can follow once their output is qualified. [Pandoc citation syntax][pandoc] · [Earlier WrOS export and sync plan][earlier] · [rc22 design][rc22] · [Current export](../../../crates/grimoire-core/src/export.rs)

**MLA 9 first.** Hayagriva 0.9.1 has a CSL-JSON input feature and adapter. Typst documents the MLA 9 style name. The CSL repository’s `modern-language-association.csl` identifies itself as MLA Handbook 9th edition, in-text citations. Pin that file and an English locale. On paper, the style has the pieces needed for the MLA examples, including author and locator handling and bibliography formatting. Reading the XML cannot verify the renderer. A Hayagriva report describes missing final periods in MLA 9 output without establishing the cause. I could not confirm output quality on 0.9.1 here. [Hayagriva manifest][hay] · [Pinned adapter][hay-json] · [Typst bibliography styles][typst] · [MLA 9 CSL code][mla-csl] · [Reported punctuation problem][hay-215]

Make MLA’s examples acceptance fixtures. For Naomi Baron’s journal article, expect `(Baron 194)` and, with author in prose, `(194)`. Verify the Works Cited entry’s author, quoted article title, italic journal title, volume, issue, date, page range, hanging indent and terminal period. Add a book, a chapter with editors and a container, a paginated journal article with DOI, a no-author webpage, and two works by one author. Verify short titles where needed and punctuation around the citation. Compare Word, PDF and Markdown with MLA’s examples and off-device Pandoc output. If Hayagriva fails, isolate the adapter, engine or style issue before shipping. Use the earlier report’s fallback decision. Qualify APA and Chicago later. [MLA in-text examples][mla-in-text] · [Works Cited guide][mla-works] · [Format examples][mla-formats] · [MLA page range and DOI][mla-doi] · [Pandoc citeproc][pandoc] · [Earlier WrOS report][earlier]

## How it fits Grimoire

- **At the cursor.** Add **Edit › Insert citation here…** beside the existing footnote action. Add **Insert citation at the cursor** to `Ctrl-K`. The overlay searches project snapshots first. Offer **Search Zotero** as an explicit network action. Send the search text the writer enters, follow `Link` pagination as needed, and allow results to be limited to a collection. Use arrows, `Enter`, `Tab` for optional locator and `Esc` to return. Keep the manuscript cursor and view. Reuse the existing editor action dispatch and footnote insertion gates. [Menu](../../../src/app/overlays/bar.rs) · [Palette](../../../src/palette.rs) · [Dispatch](../../../src/app.rs) · [Key routing](../../../src/main.rs) · [API search and pagination][api]
- **In Settings.** Add **Backups and accounts › Zotero** for Connect, account status, refresh and sign-out. Add **Writing › Citations** for the project style and lookup preferences. Opening Settings simply shows local state. The existing route map already drives grouped pages and breadcrumbs. Add the entries there and use the existing export review (`p`) before export (`x`). [Settings routes](../../../src/app/shell.rs) · [Settings help](../../../assets/help/23-settings.md) · [Export keys](../../../assets/help/18-compile.md)
- **Beside the writing.** Put **Project sources…** in Tools › Notes and the palette. The first release can use a full-width source list and form. A later **View › Sources beside…** can reuse right-side layout and focus conventions. The current `BesidePane` displays a path plus scroll state. Add a source-list state with its own selection. The renderer gives the editable second pane priority and shows Codex or Beside only when there is room. Offer the same list full-width at 80×24. Preserve `Ctrl-O`, `F11`, focus mode and existing global shortcuts. [Beside state and focus](../../../src/app.rs) · [Pane layout](../../../src/ui.rs) · [Beside help](../../../assets/help/09-a-scene-beside.md) · [Keys](../../../assets/help/24-every-key.md)
- **Library management.** Add **Tools › Zotero library…** and a palette action once the library has its own offline store. Use a full-screen overlay in the existing Grimoire process, with collection, result and detail views. Stack them on narrow screens. `Tab` switches regions, arrows navigate, `Enter` opens, and `Esc` returns to the saved writing location. Keep account settings in Settings and editing in the library view. Use the rc22 project names and resolve older folders with the shared path rules. [Overlay dispatch](../../../src/app/overlays/mod.rs) · [Menu](../../../src/app/overlays/bar.rs) · [rc22 design][rc22]

## Laptop data, security, and network

**Release one.** Cache recent searches and chosen metadata only, with a 4 MiB target and an explicit clear action. A small bounded JSON store is adequate. Keep project records after cache eviction and account removal. Use normal project save notifications for `sources.json` and any origin sidecar. Carry them in backup, timeline and folder import. Drive needs merge-base and conflict support for these reserved paths. [Earlier WrOS data plan][earlier] · [Sync helper](../../../os/scripts/wros-sync) · [Timeline implementation](../../../crates/grimoire-core/src/sessions.rs)

**Full metadata client.** Use a WrOS-owned SQLite database in the writer-only Zotero data directory, per account. Keep raw editable item JSON, searchable projections, library and object versions, collection membership, outbox, tombstones and conflicts. Use SQLite transactions for durable page batches and checkpoints. Use WAL with a bounded checkpoint policy. Consider FTS5 after measuring title, creator and tag lookup. Keep SQLite and its journal off shared folders. A complete JSON file would force larger rewrites and wider conflict recovery. Keep project snapshots portable JSON. [SQLite WAL][sqlite-wal] · [SQLite FTS5][sqlite-fts] · [Zotero sync properties][sync] · [Zotero direct database guidance][direct-db]

| Estimated metadata footprint | 1,000 Zotero objects | 10,000 Zotero objects |
| --- | --- | --- |
| Raw JSON at 2–6 KiB per object | 2–6 MiB | 20–59 MiB |
| Steady SQLite budget with indexes and limited saved bases | 5–20 MiB | 50–200 MiB |
| Initial fetch batches at 50 plus checks, assuming every object is new | About 20 data requests plus version, group and deletion requests | About 200 data requests plus version, group and deletion requests |

These are planning estimates. Count notes, annotations and attachment metadata as objects. Long notes and many annotations can exceed them. Reserve room for WAL and temporary data during migration. One thousand 5 MiB PDFs would use about 4.9 GiB by themselves. For optional file support, start at a 256 MiB evictable cache with per-file limits, space checks and explicit pins. Keep its usage visible. [Sync batching][sync] · [WAL behavior][sqlite-wal] · [Data and file sync][data-sync]

On the HP Stream’s 4 GB RAM and 32 GB eMMC, load result windows and one detail record at a time. Use one bounded background worker and batch disk writes. Start with explicit refresh. For full sync, allow one check on reconnect or when opening the library and a user-enabled idle interval. Defer first pulls on battery saver or metered links unless requested. Skip a permanent connection in the first two releases. Profile 1,000 and 10,000 objects, a mixed note fixture, editor latency, disk writes, cold and warm open, long idles, and suspend/resume. Target under 32 MiB extra peak RSS for the picker and under 64 MiB for the metadata worker at 10,000 objects. Measure release closure and signed update deltas against both retained OS generations. These targets are unmeasured. [WrOS qualification](../../NIXOS_QUALIFICATION.md) · [Earlier footprint plan][earlier] · [Zotero streaming option][stream]

**Network documentation for the implementation.** Extend `docs/API_AND_NETWORK_USE.md` with each host, trigger, data type and credential. Release one reaches the WrOS HTTPS relay during explicit pairing. The phone and relay use `www.zotero.org` for consent and the exchange. The laptop uses `api.zotero.org` for key validation, explicit search terms, selected metadata reads and supported revocation. Add group IDs and versions in the offline library release, then item changes and deletes in the write release. If WebSocket notifications ship, include `stream.zotero.org` and when it connects. Registry lookups send the explicit identifier to `api.crossref.org`, `api.datacite.org`, `openlibrary.org` or `export.arxiv.org`. Record attachment transfer hosts after inspecting real redirects, and record the writer-configured HTTPS WebDAV host if supported. Explain the relay’s temporary role and how it expires data. Keep manuscript text and project paths on the laptop through Zotero linking and citing. The writer’s separate project Drive settings govern the sidecars. [Network audit](../../API_AND_NETWORK_USE.md) · [Zotero API][api] · [OAuth][oauth] · [Streaming API][stream] · [File API][file-api] · [Crossref][crossref] · [DataCite][datacite] · [Open Library][openlib] · [arXiv API][arxiv]

Use an unprivileged Grimoire worker. The proposed flow needs outbound HTTPS only on the laptop. No polkit action or inbound laptop firewall rule is needed. Reject untrusted pagination hosts, never forward the Zotero key across a redirect, bound response sizes, and treat remote text and HTML as data. Keep the relay’s app secret outside the build; give the short-lived session no access to the laptop’s other accounts or data. If adding file transfers later, isolate parsing from the editor and retire canceled jobs. [Network module](../../../nixos/modules/network.nix) · [Polkit rules](../../../nixos/modules/polkit.nix) · [API HTTPS and pagination][api] · [Zotero privacy policy][privacy]

## Alternatives with accounts

| Service | Login and API evidence | Fit |
| --- | --- | --- |
| **Mendeley (Elsevier)** | Its developer portal still documents OAuth authorization-code login with refresh tokens and document, annotation and deletion endpoints. It also has an app registration page. I could not confirm live registration or a new grant. Review Elsevier’s Mendeley-specific terms before choosing it. [OAuth][mendeley-auth] · [Registration][mendeley-reg] · [Endpoints][mendeley-api] · [Terms][mendeley-terms] | Best second candidate if a writer already uses it. A new provider still needs a browser callback design and separate sync and metadata mapping. The owner’s library preference favors Zotero. |
| **EndNote (Clarivate)** | EndNote offers an account login. Clarivate labels the EndNote API deprecated and closed to new subscriptions. Existing subscriptions remain valid. [EndNote login][endnote-login] · [Developer portal][endnote-api] | Rule out for a new integration. |
| **Paperpile** | Its platform beta offers a REST API, CLI, and MCP with account-issued API keys. The public REST API is listed as in progress. I could not confirm general access or a delegated user-login API. [Platform beta][paperpile-beta] · [Roadmap][paperpile-roadmap] | Rule out for the first release. Its currently documented developer flow brings back manual keys and beta enrollment. |
| **Readwise Reader** | Its API returns reading documents and allows saved documents and highlights to be retrieved with a user’s access token. [Reader API][readwise] · [Highlight API][readwise-highlights] | Useful for a future clipping import. Rule out as the bibliographic library and sign-in integration. |
| **BibSonomy** | The service publishes a REST API. JabRef’s implementation asks for the writer’s username and API key. I found no established delegated sign-in flow for WrOS. [BibSonomy’s API posts][bibsonomy] · [JabRef API-key prompt][jabref-bibsonomy] | Plausible bibliographic service. Rule out for the owner’s login requirement. |
| **ReadCube Papers** | The vendor documents its library and export workflow. I could not confirm public third-party login and personal library sync APIs. [ReadCube library guide][readcube] | Keep file import as the route for writers who use it. |

## Phases and proof

Estimates are for one WrOS developer and include tests. File paths shown for new modules are proposals. External response and service setup time is separate.

| Phase | Work and files | What proves it | Effort and size |
| --- | --- | --- | --- |
| **0. Login and MLA spikes** | Discuss scoped first-party login and OAuth revocation with Zotero. Register an OAuth prototype. Prototype a separate WrOS HTTPS relay and `src/zotero/auth.rs`. Pin sample CSL and locale assets. Try `crates/grimoire-core/src/citations.rs`, `export.rs`, core `Cargo.toml` and `Cargo.lock`. | Different-network phone and laptop, clean browser, account with two-step login, grant scope, denial, expired/replayed callback, cancel, successful and failed revocation. MLA example goldens, off-device comparison, rendered Word/PDF proof and pinned-device closure/RSS baseline. | 1–2 weeks plus provider response. Estimate 2–10 MiB added installed closure for the first citation/transport build, including style files. Relay footprint separate. |
| **1. Smallest useful release** | Phone connection, personal library search/selection, recent cache, project snapshots, manual entry, CSL-JSON import, page locator, MLA 9 Word/PDF and Markdown handoff. Add `src/zotero/{mod,auth,api}.rs` and `src/app/overlays/citations.rs`. Wire `src/{app,main,palette,ui}.rs`, `src/app/{shell,backup,timeline}.rs`, and `src/app/overlays/{bar,mod,readers,import,conflicts}.rs`. Extend `crates/grimoire-core/src/{lib,project,citations,export,preflight}.rs`, `os/scripts/wros-sync`, `nixos/pkgs/rust-packages.nix`, Help and `docs/API_AND_NETWORK_USE.md`. | On the HP Stream, link, cite three sources, unplug the network, reopen, and export. Prove keyboard and narrow layout, Undo, stable keys, refresh preview, offline file import, permissions, no credentials in backup/Drive, and network silence when idle. Cover interrupted save and incomplete Drive sync. Use `src/{bar_ui_tests,shell_ui_tests,ui_tests}.rs`, `tests/driven.rs`, `os/tests/test_sync.py` and the existing import and timeline VM tests. If pairing is blocked, release the manual/file workflow as an offline milestone and report linking as pending. | 4–6 weeks after the spike. Aim for at most 16 MiB added installed closure and a 4 MiB account cache. |
| **2. Offline library, read-only** | Add `src/zotero/{store,sync}.rs` and `src/app/overlays/zotero.rs`. Add SQLite, portable migrations, read-side incremental sync, personal and opted-in group collections and tags, child note previews, trash view and local search. Add the library view and Settings scope controls in the same UI files. Update manifests, packaging and network docs. | 1,000/10,000 objects with concurrent server activity, page failure, restart, deletion, withdrawn group membership, `304`, `429`, backoff and disconnection. Confirm bounded memory and disk and compare a disposable fixture library to a stock Zotero client. | 3–5 weeks. Budget 2–8 MiB further closure for SQLite and bindings if needed. Use the storage ranges above. |
| **3. Two-way metadata** | Extend `src/zotero/{api,store,sync}.rs` with durable outbox, permissions and a conflict overlay. Add edit forms, manual item creation, versioned trash and restore, safe note editing, and separate `src/source_lookup.rs`. Extend account scopes, menus and network docs. Qualify additional export styles and EPUB/paperback as needed in `grimoire-core` export modules. | Tests with two clients, offline edits, group read-only roles, access changes, timeout after commit, simultaneous different and same field edits, delete versus edit, malformed HTML and notes round-trip. Verify no duplicate or lost optimistic writes after resume. Test source lookup fixtures and prove export remains offline. | 5–9 weeks. Plan for low single-digit MiB further closure and disk headroom for unsent edits and bases. Added styles are a separate qualification cost. |
| **4. Files and annotation parity, if wanted** | Add `src/zotero/files.rs`, a private download/cache store, then Zotero Storage upload if needed. Spike text preview before a pixel viewer. Touch graphics only for the pixel option. Scope a separate WebDAV backend, annotation editor, Help, packaging and network docs. | Real personal and group test libraries, quota refusal, redirects, resume, integrity, eviction, encrypted or malformed PDFs and scanned pages. Round-trip note and annotation data with official clients. Measure storage, package size and battery on the laptop. | 3–5 weeks for bounded download/text preview. Add roughly 6–12 or more weeks for upload, WebDAV, pixel viewer and annotation editing. Closure impact unmeasured. Budget at least a separate 256 MiB evictable file cache if enabled. |

## Risks and what remains unconfirmed

- **Login is the largest dependency.** Verify a registered WrOS callback, actual read-only grant behavior, key deletion for OAuth grants, and phone-to-laptop completion. Ask Zotero about first-party-style sessions and the developer-list scope report. I did not enroll an account, register an app, or run any live auth exchange. Check terms with Zotero if WrOS will sell or otherwise commercially distribute the integration. Section 6 calls for prior written permission for commercial or promotional exploitation of the services. The developer-list question I found had no visible staff answer. [OAuth][oauth] · [Terms][terms] · [Commercial-use thread][commercial] · [Scope report][scope-report]
- **MLA correctness is a release gate.** I checked the style definition, adapter and examples. I could not run Hayagriva or confirm punctuation, typography, export layout or real device footprint. Treat the estimates as budgets to test. [MLA 9 style][mla-csl] · [Hayagriva punctuation report][hay-215] · [MLA examples][mla-in-text]
- **Two-way sync raises the cost of a bug.** A stale checkpoint, discarded conflict or negligent retry can damage the writer’s remote library. Qualify read-only first and enable writes per library. Test lost group access, interrupted refresh and failed server writes. [Sync algorithm][sync] · [Write API][writes]
- **File work can dominate the appliance budget.** WebDAV behavior, current download hosts, PDF rendering in both WrOS display paths and rich note and annotation fidelity are untested. Recheck licences for the exact distribution and dependencies before copying code or adding a runtime. [Storage and WebDAV][data-sync] · [File API][file-api] · [Zotero licensing][z-license] · [Earlier WrOS licence review][earlier]

## Questions for the owner

1. Is scanning a QR and approving on zotero.org the regular login you want? If Zotero offers no supported device login, would you operate the small callback service?
2. Is personal, read-only selection enough for now? Which actions would make you open a Zotero library inside WrOS instead of on your phone or another computer?
3. Can we use one of your MLA 9 papers as an acceptance fixture? Which export formats do you actually turn in, and do recipients need editable citation fields in Word?
4. Would you use shared libraries, note editing or PDFs on the HP Stream? If files matter, are they in Zotero Storage or WebDAV, and would text preview cover the common task?

## Sources

- WrOS scope and integration points. [Earlier citation report][earlier], [rc22 project naming][rc22], [network audit](../../API_AND_NETWORK_USE.md), [Google phone-link setup](../../GOOGLE_SIGN_IN_SETUP.md).
- Zotero service and client behavior. [Public API index][api-index], [authentication][api], [OAuth][oauth], [terms][terms], [desktop browser login change][desktop-web-login], [staff on revocation][staff-password], [sync][sync], [file transfers][file-api] and [licensing][z-license]. Individual source files and client repositories are linked where used.
- Style and export. [MLA Handbook 9][mla-handbook], [MLA citation examples][mla-formats], [MLA CSL][mla-csl], [pinned Hayagriva manifest][hay], [CSL-JSON input schema][csl-json] and [Pandoc citations][pandoc]. Provider sources are linked in the alternatives table.

[earlier]: 2026-11-21-citation-management-research.md
[rc22]: 2026-11-21-rc22-projects-youtube-keys-design.md
[api-index]: https://www.zotero.org/support/dev/web_api/v3/
[api]: https://www.zotero.org/support/dev/web_api/v3/basics
[oauth]: https://www.zotero.org/support/dev/web_api/v3/oauth
[terms]: https://www.zotero.org/support/terms/terms_of_service
[privacy]: https://www.zotero.org/support/privacy
[z-license]: https://www.zotero.org/support/licensing
[z-source]: https://www.zotero.org/support/dev/source_code
[z-news]: https://www.zotero.org/support/changelog
[desktop-9-api]: https://github.com/zotero/zotero/blob/9.0.6/chrome/content/zotero/xpcom/sync/syncAPIClient.js#L526-L605
[desktop-9-news]: https://www.zotero.org/support/9.0_changelog
[desktop-web-login]: https://github.com/zotero/zotero/commit/1502214b5d83115ac1290a4f4996898b1411e359
[desktop-runner]: https://github.com/zotero/zotero/blob/387f61df219d76dce599479cb531e621a2f55c76/chrome/content/zotero/xpcom/sync/syncRunner.js#L1619-L1676
[desktop-sync-code]: https://github.com/zotero/zotero/tree/main/chrome/content/zotero/xpcom/sync
[mfa]: https://www.zotero.org/support/kb/multifactor_auth
[staff-storage]: https://forums.zotero.org/discussion/102961/where-is-the-password-stored
[staff-old]: https://forums.zotero.org/discussion/134451/desktop-sync-login-returns-invalid-username-or-password-403-from-keys-but-website-login-works
[staff-verifier]: https://forums.zotero.org/discussion/113309/oauth-down
[staff-secret]: https://forums.zotero.org/discussion/110819/oauth-authorization-without-storing-the-secret-key
[staff-password]: https://forums.zotero.org/discussion/120485/if-computer-gets-stolen-will-changing-my-password-prevent-changes-to-my-library
[staff-security]: https://forums.zotero.org/discussion/129670/somebody-elses-library-on-my-zotero-account
[staff-failures]: https://forums.zotero.org/discussion/134061/
[staff-rates]: https://forums.zotero.org/discussion/comment/499834/
[scope-report]: https://groups.google.com/g/zotero-dev/c/vA6FEMQ4XU8
[dev-list]: https://groups.google.com/g/zotero-dev
[commercial]: https://groups.google.com/g/zotero-dev/c/dSH6Ku0ka9k
[oauth-rfc]: https://datatracker.ietf.org/doc/html/rfc5849#section-2.1
[zoteror]: https://github.com/giocomai/zoteror#create-an-api-key-and-store-credentials
[local-api]: https://www.zotero.org/support/dev/web_api/v3/local_api
[sync]: https://www.zotero.org/support/dev/web_api/v3/syncing
[writes]: https://www.zotero.org/support/dev/web_api/v3/write_requests
[stream]: https://www.zotero.org/support/dev/web_api/v3/streaming_api
[data-sync]: https://www.zotero.org/support/sync
[groups]: https://www.zotero.org/support/groups
[tags]: https://www.zotero.org/support/collections_and_tags
[storage]: https://www.zotero.org/storage
[attachments]: https://www.zotero.org/support/attaching_files
[attachment-download]: https://github.com/zotero/zotero/blob/387f61df219d76dce599479cb531e621a2f55c76/chrome/content/zotero/xpcom/storage/zfs.js
[file-api]: https://www.zotero.org/support/dev/web_api/v3/file_upload
[webdav]: https://www.zotero.org/support/kb/webdav_services
[formats]: https://www.zotero.org/support/dev/data_formats
[export-fidelity]: https://forums.zotero.org/discussion/122740/which-export-format-via-api-returns-the-most-complete-dataset
[direct-db]: https://www.zotero.org/support/dev/client_coding/direct_sqlite_database_access
[add-items]: https://www.zotero.org/support/adding_items_to_zotero
[notes]: https://www.zotero.org/support/notes
[note-code]: https://github.com/zotero/zotero/blob/387f61df219d76dce599479cb531e621a2f55c76/chrome/content/zotero/xpcom/data/notes.js
[annotations]: https://www.zotero.org/support/kb/annotations_in_database
[annotation-code]: https://github.com/zotero/zotero-ios/blob/affbf9dea91e9adedd8192244a3b38eaa4e83d83/Zotero/Controllers/Database/Requests/CreateOrEditPDFAnnotationsDbRequest.swift
[ios]: https://www.zotero.org/support/ios
[android]: https://www.zotero.org/support/android
[ios-src]: https://github.com/zotero/zotero-ios
[android-src]: https://github.com/zotero/zotero-android
[ios-commit]: https://github.com/zotero/zotero-ios/commit/affbf9dea91e9adedd8192244a3b38eaa4e83d83
[android-commit]: https://github.com/zotero/zotero-android/commit/32a9265495e7c4645a735bbdfdedef875c1f33d7
[ios-login]: https://github.com/zotero/zotero-ios/blob/affbf9dea91e9adedd8192244a3b38eaa4e83d83/Zotero/Scenes/Onboarding/ViewModels/LoginActionHandler.swift
[android-login]: https://github.com/zotero/zotero-android/blob/32a9265495e7c4645a735bbdfdedef875c1f33d7/app/src/main/java/org/zotero/android/api/AuthApi.kt
[zoo]: https://github.com/mickstar/Zoo-For-Zotero/blob/02610408d54b63856831cdbf28247fcadc9ef26e/app/src/main/java/com/mickstarify/zooforzotero/SyncSetup/ui/components/ZoteroOAuthWebView.kt
[papership]: https://forums.zotero.org/discussion/117743/papership-now-unable-to-log-in
[zotfile]: https://zotfile.com/
[obsidian]: https://github.com/community-archive/obsidian-zotero-integration
[zotflow]: https://github.com/duanxianpi/zotflow
[papis-zotero]: https://github.com/papis/papis-zotero
[connector-callback]: https://github.com/zotero/zotero-connectors/blob/master/src/common/inject/inject.jsx
[pyz-pin]: https://pypi.org/project/pyzotero/1.11.0/
[pyz-doc]: https://pyzotero.readthedocs.io/en/stable/index.html
[pyz-commit]: https://github.com/urschrei/pyzotero/commit/dc6525c6ef0d08f99b2a785d9ac491c0aedcbe66
[zotcli]: https://pypi.org/project/zotcli/
[rust-cli]: https://github.com/riccardo-enr/zotero-cli
[rust-manifest]: https://github.com/riccardo-enr/zotero-cli/blob/main/Cargo.toml
[rust-commit]: https://github.com/riccardo-enr/zotero-cli/commit/fbc1761e41ae348732521ebdb046011890aa5696
[old-cli]: https://github.com/jbaiter/zotero-cli
[old-cli-commit]: https://github.com/jbaiter/zotero-cli/commit/273fb9b69223585775106baafca061bee9403e16
[zui]: https://github.com/camilo-zuluaga/zui
[zui-commit]: https://github.com/camilo-zuluaga/zui/commit/ddcbf5c9db3ede733c80aba79c9b015d6576c4fe
[zui-sync]: https://github.com/camilo-zuluaga/zui/blob/master/sync/sync.go
[refero]: https://github.com/chu-/Refero
[refero-commit]: https://github.com/chu-/Refero/commit/14e2611f7e121b6b1b87ae3f5f24b1924016496d
[emacs]: https://github.com/emacsmirror/zotero
[emacs-commit]: https://github.com/emacsmirror/zotero/commit/eef5080e6a2ed0cae12c3d21580864f4b394cd5f
[ret-sync]: https://github.com/retorquere/zotero-sync
[ret-sync-manifest]: https://github.com/retorquere/zotero-sync/blob/9f2a76b15dc7937c74a3294003d085fca46aafe3/package.json
[crossref]: https://www.crossref.org/documentation/retrieve-metadata/rest-api/
[datacite]: https://support.datacite.org/docs/api
[openlib]: https://openlibrary.org/dev/docs/api/books
[arxiv]: https://info.arxiv.org/help/api/user-manual.html
[translation]: https://github.com/zotero/translation-server
[pdftotext]: https://manpages.debian.org/bookworm/poppler-utils/pdftotext.1.en.html
[pdftoppm]: https://manpages.debian.org/bookworm/poppler-utils/pdftoppm.1.en.html
[hay]: https://github.com/typst/hayagriva/blob/v0.9.1/Cargo.toml
[hay-json]: https://github.com/typst/hayagriva/blob/v0.9.1/src/csl/taxonomy.rs#L675
[hay-215]: https://github.com/typst/hayagriva/issues/215
[typst]: https://typst.app/docs/reference/model/bibliography/
[csl-json]: https://github.com/citation-style-language/schema/blob/master/schemas/input/csl-data.json
[mla-csl]: https://github.com/citation-style-language/styles/blob/master/modern-language-association.csl
[mla-handbook]: https://style.mla.org/mla-handbook-ninth-edition/
[mla-in-text]: https://style.mla.org/in-text-citations-overview/
[mla-works]: https://style.mla.org/works-cited/works-cited-a-quick-guide/
[mla-formats]: https://style.mla.org/works-cited/citations-by-format/
[mla-doi]: https://style.mla.org/page-range-and-doi/
[pandoc]: https://pandoc.org/MANUAL.html#citations
[sqlite-wal]: https://sqlite.org/wal.html
[sqlite-fts]: https://sqlite.org/fts5.html
[mendeley-auth]: https://dev.mendeley.com/reference/topics/authorization_auth_code.html
[mendeley-reg]: https://dev.mendeley.com/reference/topics/application_registration.html
[mendeley-api]: https://dev.mendeley.com/methods/
[mendeley-terms]: https://dev.mendeley.com/terms-and-conditions.html
[endnote-login]: https://endnote.com/login/
[endnote-api]: https://developer.clarivate.com/apis/endnote
[paperpile-beta]: https://join.paperpile.com/h/agent-plugins-mcp-beta/
[paperpile-roadmap]: https://paperpile.com/roadmap/
[readwise]: https://readwise.io/reader_api
[readwise-highlights]: https://readwise.io/api_deets
[bibsonomy]: https://blog.bibsonomy.org/search/label/api?m=0
[jabref-bibsonomy]: https://github.com/JabRef/jabref/blob/main/jablib/src/main/resources/l10n/JabRef_en.properties
[readcube]: https://www.readcube.com/wp-content/uploads/2024/09/ReadCube-Getting-Started-Library-Fall-2024.pdf
