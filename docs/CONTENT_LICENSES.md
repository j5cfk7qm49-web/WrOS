# Content licensing and attribution

This is an engineering rights inventory, **not a legal opinion or worldwide public-domain certification**. Machine-readable source of truth: [content-licenses.json](content-licenses.json). Software and dictionary notices are separate in [software-licenses.json](software-licenses.json) and `assets/dict/DICTIONARY-LICENSE`.

## Actually bundled creative/tutorial content

The 39 tutorial instructions, two short demo scenes, thirty M5 exercises and M6 original Spark bundle are under repository MIT terms to the extent rights subsist. M6 contains 1,047 accepted original plots and 84 exercises with AI-assisted drafting and agent authoring/editorial review; four additional held plot drafts remain in source, not the runtime count. Every accepted record pins edition/section/file/line/file SHA-256/text SHA-256 and adaptation/attribution. No independent human/legal or exclusive-copyright claim is made. No third-party creative exercise, historical book body or modern prompt list is bundled or counted. Existing Hunspell dictionaries have their retained SCOWL/Ispell notices; they are software/reference data, not a creative prompt license shortcut.

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

`python3 tools/generate-writingdeck-manifests.py` uses cached `cargo metadata --locked --offline`, retains Rust package notices in `os/licenses/RUST-THIRD-PARTY-NOTICES.txt`, hashes bundled original/dictionary files, and inventories the **staged guest** Debian packages (not host packages). It marks which resolved crates belong to the Linux no-audio build. The current generated all-platform/default-feature Rust inventory lists the actual resolved records; 71 all-platform published archives lack root-level retained notice files and remain review items; **none of these is used by the current Linux no-audio image**. The all-platform inventory is a superset, not a claim every crate is linked. Full OS corresponding-source/release review remains open. Regenerate after dependency/content changes and review the diff; do not use a generator to override unclear third-party rights.

## M3b content review

Lesson 13 now instructs an actual safe-demo secondary edit; its wording is original MIT project text, as are the unchanged two demo scenes. Regenerate the bundled tutorial/demo hashes after reviewing this catalog change. No evaluated third-party candidate has been promoted to bundled status by adding the document engine or these tests.

## M6 original content decision

[CHALLENGE_LIBRARY](CHALLENGE_LIBRARY.md) and [spark-corpus-manifest.json](spark-corpus-manifest.json) distinguish all 1,051 literal plot drafts from 1,047 accepted situations; holds 0544/0679/0922/0986 are substantive-overlap decisions, not renumbered slots. The compiler validates stable IDs, normalized duplicates and exact provenance and publishes fixed editorial/lexical samples; none of that is independent human/legal attestation. `legal_clearance` and distribution readiness remain false.

Cook's exact 1928 Archive OCR/XML research is recorded but not ingested into runtime. Sampled extraction defects and territory/narrative-span gaps led to independently worded originals instead. A modern cleaned transcript's absent license was not inferred from US public-domain status. All evaluated third-party candidates remain unbundled. No CC BY-SA content was relabeled MIT. Lessons 22–24 instructions are new original project wording, and the license registry pins their current catalog hash.

## M7a packaged models and omitted notices

Harper/Brill/POS 2.13 is now linked as an **optional disclosed local model-based grammar service**, not merely evaluated. [model-licenses.json](model-licenses.json) pins nine packaged dictionary/annotation/tagger/chunker files by hash/size and locked crate archive/VCS identity. Root Apache-2.0 notices omitted by the three Harper archives, and MIT/Apache texts omitted by five selected Burn/CubeCL/Embassy archives, are retained verbatim at their exact packaged VCS revisions; [upstream-notice-sources.json](upstream-notice-sources.json) records URLs and hashes. The generated installed Rust bundle contains them. Run `python3 tools/audit-writing-checks-assets.py` offline against the fetched locked graph; it is a verifier gate.

This is integrity/upstream-declaration review, **not independent training-data/inherited-dictionary provenance, legal clearance or complete corresponding-source certification**. `distribution_ready` remains false; the package-wide Apache declaration is not silently promoted into such an attestation. Model review is distinct from creative-prompt rights. The original supplemental lesson-40 wording is project MIT text, not imported third-party exercise content.
