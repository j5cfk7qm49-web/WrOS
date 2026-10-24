# Offline writing and revision tools

## Current functionality versus planned integrations

Current: spellbook/Hunspell locale spelling, personal/notebook vocabulary, suggestions, notes/TK handling, find/replace, echo/revision aids, scene history and sentence-level local TTS are preserved. Harper, Nabokov, proselint integration, offline WordNet UI, voice profiles and a unified revision dashboard are **Planned**, not included merely because a host package was installed for evaluation.

## Evaluated dependencies

| Engine/resource | Evaluated version/license | Runtime and integration decision |
|---|---|---|
| `harper-core` | 2.13.0, Apache-2.0 | Rust 2024, embeddable/offline. Curated/FST dictionary, mutable/merged personal vocabulary, `Document`, `LintGroup`, overlap helpers. Preferred grammar layer alongside existing spelling, not a silent replacement |
| `viewflow/nabokov` | source pyproject 26.7.10, MIT | Python ≥3.12, spaCy ≥3.8,<3.9, separately installed `en_core_web_sm`; JSON output. Optional worker/deep analysis, no `--ai` requirement. Pin source/model hashes and inspect model license before bundling |
| `amperser/proselint` | current source 0.16.0, BSD-3-Clause | Python ≥3.10; standalone CLI, not Vale. Current source uses `proselint check`; Debian 13's evaluated 0.14.0 uses the older `proselint --json` shape. Detect/pin version and test both offset schemas rather than assume compatibility |
| Princeton WordNet / Open English WordNet | WordNet permissive notice / OEWN CC BY 4.0 + inherited attribution | Prefer maintained Debian WordNet data for first definitions/senses/antonyms UI; Harper's thesaurus alone is not a full dictionary |
| Piper/eSpeak | Engines evaluated in existing TTS code; voices have separate licenses | Keep local engines optional; no unlicensed model or cloud voice dependency |

Source links: <https://crates.io/crates/harper-core/2.13.0>, <https://github.com/Automattic/harper>, <https://github.com/viewflow/nabokov/blob/main/pyproject.toml>, <https://github.com/amperser/proselint/blob/main/pyproject.toml>, <https://wordnet.princeton.edu/>, <https://github.com/globalwordnet/english-wordnet>. License metadata and bundling status are recorded separately in `software-licenses.json`.

## Common diagnostics design

A diagnostic carries engine/rule/category/severity, document identity and revision hash, original-source UTF-8 byte start/end, message/suggestions, narration/dialogue/notes/research context and suppression key. Engines that report characters or line/column offsets must map through tested Unicode source tables. Markdown stripping, curly quotes and footnotes must not shift highlights onto unrelated prose.

Run revision-tagged jobs off the input thread; discard stale results and deduplicate overlapping equivalent findings. Give Harper correctness checks precedence, Nabokov grammatical passive analysis precedence and proselint specialized usage/cliché checks precedence. Keep all provenance and rule-specific toggles. Apply a suggestion only after explicit writer action, never silently.

Drafting: spelling/major grammar, low interruption. Revision: selected readability/style patterns. Deep Analysis: explicit broader rhythm/repetition/voice analysis. Quoted fictional dialogue remains prose, not an external quotation to suppress wholesale. Notes and research have their own policies and do not affect novel statistics by default.

## Respecting literary style

Do not call every passive, adverb, fragment, repetition or long sentence an error. Show patterns of possible accidental complexity, overuse and reader difficulty. Readability grade is not quality. Voice profiles use enough author-selected material and present deviations as information, not corrections. No overall literary-quality score.

## Revision and long-form extensions

The future dashboard unifies grammar/spelling/style findings, annotations, continuity questions, footnote/export errors and user tasks with filters, reviewed/suppressed states and direct navigation. Continuity uses explicit facts/scene chronology for definite contradictions; inferred concerns remain suggestions. Anchored comments/variants/named checkpoints are additional domain modules, not already supplied by dated local history.

## Footnotes, publishing and read-through

Baseline exports native DOCX, EPUB and Markdown, and PDF through optional LibreOffice. Markdown footnotes are not certified: implement reference/definition validation and genuine OOXML footnote parts, then check actual PDF page placement, numbering and cross-references. Never silently substitute endnotes. Keep annotations/scratchpad/private material out of publication.

Extend existing sentence-level local TTS with a multi-scene/chapter queue, speed/voice controls, durable optional progress and mark-for-revision. Do not claim these queue features because Piper/eSpeak already exists.

## Acceptance and performance gates

Before M7/M8/M9 release: deterministic Unicode/Markdown/dialogue offset fixtures; stale-result/cancellation/dedup tests; once/novel/global ignores and dictionary migration; WordNet sense/antonym fixtures; adequate-material voice profiles; explicit continuity truth examples; annotation relocation; reversible diff/restore; DOCX package XML plus multi-page PDF checks. Benchmark RSS/latency/idle CPU on representative low-power hardware and record Python/spaCy/model costs separately. No model download or manuscript network request may occur during normal offline checking.
