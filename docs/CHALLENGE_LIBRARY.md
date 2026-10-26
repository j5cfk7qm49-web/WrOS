# Offline original challenge library

**Bundled M6 corpus:** 1,047 agent-accepted plot situations and 84 new independently usable creative exercises, compiled from 1,051 literal plot drafts and 84 exercise drafts. This is original **AI-assisted drafting and agent authoring/editorial review**, not an independent human/editorial/legal attestation or an exclusive-copyright claim. Runtime selection is offline and non-AI. The thirty M5 exercises, tutorial scenes, historical OCR and hypothetical plot/exercise products are not counted.

## Rights-first decision

M6 uses independently worded originals, MIT **to the extent rights subsist**. No modern prompt list, historical book body or ShareAlike adaptation is included. [CONTENT_LICENSES](CONTENT_LICENSES.md) and [content-licenses.json](content-licenses.json) pin the bundle and clearly retain `legal_clearance: false`. Original source drafts, including four held records, remain in the repository under the same original-content notice.

Research examined Cook's exact 1928 Archive edition, not a modern Tin House edition. Original OCR/XML geometry extraction was rejected: sampled candidates still contained headings, merged conflicts, missing narrative spans and generic premises, with territorial review unresolved. The modern GaryKac cleanup was not treated as licensed simply because the underlying US book may be public domain. These research artifacts are not runtime content and contribute zero accepted options. Other evaluated historical/CC sources remain `bundled: false`. A future CC BY-SA adaptation would need its own separated compatible license/attribution scope, not MIT relabeling.

## Stable records and provenance

`crates/grimoire-core/assets/writingdeck/corpus/` contains fourteen original plot TSV files, the exercise TSV and explicit editorial decisions. IDs are literal and immutable: `wd-p-0001`–`wd-p-1051`, `wd-x-0001`–`wd-x-0084`. **Never rerun ID assignment or renumber held entries.** Plot rows have four fields; exercise rows nine. Existing IDs identify wording, not a generated Cartesian template.

The offline standard-library compiler `tools/build-spark-corpus.py` validates and builds:

- `crates/grimoire-core/assets/writingdeck/spark-corpus.json`, schema 1, version **`writingdeck-original-1`**, 1,131 accepted records;
- [spark-corpus-manifest.json](spark-corpus-manifest.json), exact compiled/source/decision SHA-256 values;
- [spark-corpus.json](evidence/spark-corpus.json), counts, explicit holds, fixed stratified editorial resample, thirty-two closest lexical pairs and limitations.

Every accepted record has stable ID/kind, original wording/title, source ID/edition/section, source repository file/line/file SHA-256, text SHA-256, adaptation statement, MIT expression and full AI-assisted attribution. It also carries category/genre tags, estimated starting difficulty/duration, work mode, narrative requirements and compatibility exclusions. Repository source paths are provenance locators, not paths into a writer's manuscript.

## Count, distinctness and editorial limits

The four held plot IDs **0544, 0679, 0922, 0986** remain in draft sources but are absent from runtime records. Their causal overlap decisions are documented even where lexical similarity alone was low: inconvenient feedback reclassification, removing an obstacle that supports a refuge, sound defeating intended silence, and cooling samples competing with a person's essential supply.

Count distinct causal obstacles, incompatible claims, practical dilemmas, mysteries and stakes—not adjective/name swaps, genre relabeling or a plot × exercise multiplication. Authoring review covered the drafts; the build adds normalized duplicate refusal, full ID/hash/source coverage, lexical neighbors and a fixed resample for auditable follow-up. **A passing compiler or low Jaccard similarity is not proof of semantic distinctness or independent review.** Human editorial/release qualification remains separate; do not replace that limitation with a numeric certification.

The compiler does not invent prompt wording. `python3 tools/build-spark-corpus.py --check` compares checked-in artifacts byte-for-byte. Default mode rebuilds derived artifacts from the immutable identified sources; it must not assign new IDs or alter draft wording.

## Exercises and compatibility

There are six originals in each of fourteen categories: dialogue, description, point of view, voice, structure, motivation, senses, pacing, setting, conflict, subtext, revision, formal experimentation and read-through. Revision/read-through instructions needing existing prose remain independently useful exercise-only options; fresh combined selection uses drafting exercises only.

Nine genre tags are realist, speculative, mystery, adventure, romance, family, satire, political and historical. Thirteen explicit feature tags cover scene/time/dialogue/senses/person/order oppositions. The first ten plots of part 14 have authored scene/time conditions; silent ceremony cases additionally prohibit spoken dialogue. A character speaking in a premise or a historical setting alone does not silently prohibit another form.

Lipograms, N+7, snowball sentences, restricted vocabulary, viewpoint changes, reversal, formal dialogue, sensory limitation and temporal structure can be described in original wording. A general method is not a license to copy a modern explanation. The corpus does not implement a destructive N+7 transformer or claim a licensed lexical ordering exists when it does not.

## Integration and evidence

[STORY_SPARK](STORY_SPARK.md) documents four actual modes, compatible rerolls, local-date freeze, hard/recent pool policy, bounded reference-only history/favorites/dismissals, guarded original Markdown exports and real M4/M5 actions. Outline/task captures are expressly Markdown notes, not an invented M8 database. Creative completion is optional and self-reported, separate from measured M5 criteria. Tutorials 22–24 now require genuine corresponding persisted actions in the safe demo; they do not certify that a person completed a writing assignment.

Core and UI tests verify exact accepted IDs/provenance/compatibility/selection/recovery. Six Python corpus tests verify source lines/hashes, literal draft IDs and four holds, exact artifact rebuilds, category/work-mode coverage, exclusion manifests and license-registry alignment. Compiled PTYs exercise actual source editing/restart. These are automated software/content-integrity evidence, not independent human rights, editorial or physical-device clearance.
