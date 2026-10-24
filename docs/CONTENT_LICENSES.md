# Content licensing and attribution

This is an engineering rights inventory, **not a legal opinion or worldwide public-domain certification**. Machine-readable source of truth: [content-licenses.json](content-licenses.json). Software and dictionary notices are separate in [software-licenses.json](software-licenses.json) and `assets/dict/DICTIONARY-LICENSE`.

## Actually bundled creative/tutorial content

The 39 tutorial instructions and the two short demo scenes are original WritingDeck project text under the repository's MIT terms. No third-party creative exercise, historical book body or modern prompt list has yet been added to the challenge corpus. Existing Hunspell dictionaries have their retained SCOWL/Ispell notices; they are software/reference data, not a creative prompt license shortcut.

## Evaluated candidates (not bundled)

| Source / exact edition evaluated | Rights finding and decision |
|---|---|
| William Wallace Cook, *Plotto*, Ellis Publishing Co., **1928 original** | US public-domain candidate; exclude modern Tin House forewords/introductions/artwork. Before ingesting, pin the exact scan/transcription, checksum and territorial distribution decision. A modern reprint's free preview is not permission |
| Georges Polti, *The Thirty-Six Dramatic Situations*, **1916 English translation by Lucile Ray** | US public-domain candidate for that edition; do not assume all translations/annotations are PD worldwide. Pin the source/translation and exclude modern additions |
| Greever and Jones, *The Century Handbook of Writing*, PG **30294** | PG marks US public domain. The actual inspected title page is **1927**, copyright **1918**, not a guessed modern edition; retain authors/provenance and review the PG redistribution/trademark terms if using its branded file |
| Anonymous, *How to Write a Novel: A Practical Guide to the Art of Fiction*, Grant Richards, **London 1901**, PG **38887** | PG marks US public domain; inspected 1901 title page. Do not conflate the 2012 e-book release date with the book's publication date; translation/quoted additions still deserve review |
| Bailey, Simonian and Smith, *Vanguard: Exercises for the Creative Writing Classroom* (2020) | CC BY-SA 4.0 book-level notice. Audit per-chapter/third-party exceptions before adapting; retain creator/title/source/license/change notice and ShareAlike scope. Adapted material remains under a compatible ShareAlike license, separated from MIT code |
| Lein/Freeman/Lumen, *Introduction to Creative Writing* | CC BY 4.0 book-level notice, with separately credited material requiring review. Attribution, source/license link and modification indication required for covered adaptations |
| Stephen Fried, *Word into Idea: A Creative Writing Exercise*, CUNY CSI (2021) | CC BY 4.0 repository notice. Pin the actual exercise file and credit author/title/source/license/changes before redistribution |
| Established constraint methods | General ideas/methods may be independently implemented with original wording. Do not copy a modern Oulipo prompt list or assume a tradition is a blanket content license |

Verified source pages: <https://www.gutenberg.org/ebooks/30294>, <https://www.gutenberg.org/cache/epub/30294/pg30294-images.html>, <https://www.gutenberg.org/ebooks/38887>, <https://www.gutenberg.org/cache/epub/38887/pg38887-images.html>, <https://raider.pressbooks.pub/vanguard/>, <https://pressbooks.hcfl.edu/creativewriting/>, <https://academicworks.cuny.edu/si_oers/24/>.

## Ingestion gate

Every bundled item needs exact edition and file hash, source URL/section, creator, license or territorial PD basis, redistribution/adaptation permissions, attribution and retained notices, modifications and ShareAlike obligations. Unclear-rights/NC/ND/incompatible exceptions stay excluded unless separately cleared. Store third-party notices with the offline corpus, not only as a web link. Do not use cover images or a publisher's modern additions without their own rights.

A full OS also contains GPL and other independently licensed packages. The repository's MIT license is not a license for the entire Debian image. Preserve package copyright notices and satisfy corresponding-source obligations for distributed binaries. The manifests aid review but do not automatically certify a production distribution.

## Software inventory generation

`python3 tools/generate-writingdeck-manifests.py` uses cached `cargo metadata --locked --offline`, retains Rust package notices in `os/licenses/RUST-THIRD-PARTY-NOTICES.txt`, hashes bundled original/dictionary files, and inventories the **staged guest** Debian packages (not host packages). It marks which resolved crates belong to the Linux no-audio build. Of 346 all-platform/default-feature Rust records, 31 published archives lack root-level notice files and remain review items; **none of these is used by the current Linux no-audio image**. The all-platform inventory is a superset, not a claim every crate is linked. Full OS corresponding-source/release review remains open. Regenerate after dependency/content changes and review the diff; do not use a generator to override unclear third-party rights.

## M3b content review

Lesson 13 now instructs an actual safe-demo secondary edit; its wording is original MIT project text, as are the unchanged two demo scenes. Regenerate the bundled tutorial/demo hashes after reviewing this catalog change. No evaluated third-party candidate has been promoted to bundled status by adding the document engine or these tests.
