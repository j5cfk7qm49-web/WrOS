# M7b prose, dictionary, voice and revision tools

The recovered implementations now have fresh M7b software, native UI/PTY,
benchmark and VM-contract evidence on `codex/m7b-qualification` / PR #15.
[STATUS](STATUS.md) records the exact 910-tests-per-configuration / 70-Python
results and the explicit trained-worker CPU-denial boundary in emulation.
Production, hardware and actual remote-CI qualification remain separate gates.

## Implemented scope

- `prose.rs`: bounded native English readability, sentence/paragraph patterns,
  repetition and fiction profiles. Approximate readability is advisory and is
  withheld below 100 analysis words; it is not a literary quality score.
- `prose_tools.rs` and `prose_tools.py`: exact-version standalone proselint and
  optional Nabokov/spaCy adapters, checked Unicode/Markdown source mapping,
  stale-result rejection, child resource/deadline limits and cancellation.
  Nabokov requires a separate source/configuration-bound single-use model permit.
  Neither opening a book nor opening the dashboard authorizes model loading.
- `lexicon.rs` and the dictionary reader: local WordNet 3.0 senses, morphology,
  synonyms and sense-specific antonym pointers. Copying a word does not edit the
  manuscript; the writer may paste using the normal editor/undo path.
- `voice_profile.rs`: explicit author-selected references, stored hashes and
  measurements, no copied reference prose. Requires at least 5,000 analysis
  words, 40 segments and 150 distinct words; comparison requires 300 words.
  At most eight profiles and 64 references per profile. Differences are
  informational, never authorship detection or automatic normalization.
- `review_queue.rs` and the prose dashboard: current-source findings,
  equivalence-aware deduplication retaining engine provenance, review receipts,
  scoped mutes, filters and original-source navigation. No automatic rewrite.
- Original tutorial IDs 25 and 27–30 now have real review/comparison events.
  Opening a tool or granting permission alone does not complete a lesson.

The analyzer source cap is 65,536 bytes. Native and Python costs are separate;
Python workers are launched for explicit requests rather than kept resident.
Their kernel CPU budget is 30 seconds (32-second hard limit), with a separate
60-second wall budget. The x86-64-v2 TCG VM can reach this CPU ceiling during
Nabokov startup. Its probe records `timed-out`, an explicit CPU-budget notice,
empty findings and unchanged original bytes as a **resource-denial contract**;
it does not record a successful or clean Nabokov analysis. Positive trained
analysis is required separately in the actual native offline Rust/UI/PTY suite.
No automatic retry or relaxed application budget is used to make a VM pass.
Quoted fictional dialogue remains prose with relaxed style policy; this is not
a blanket exclusion of dialogue grammar. Existing native spelling remains
independent of optional Python/model availability.

## Target environment and reproduction

The locked Python payload targets **Debian 13, Linux amd64, CPython 3.13**.
Its pinned NumPy 2.5.4 wheel reports the **x86-64-v2** CPU baseline. Optional
Nabokov/spaCy analysis therefore needs that instruction set; merely having a
64-bit processor is insufficient. The VM probe uses QEMU's Nehalem CPU model
to exercise this baseline without AVX/AVX2. Native editing and spelling do not
depend on NumPy. An unsupported optional worker must report failure rather
than a clean check; older-CPU package variants remain separate qualification.
Use an isolated Debian environment as a normal user. It needs Rust, compiler
headers, `python3-venv`, `wordnet-base`, Git and ShellCheck. The existing macOS and
Windows editor checks do not qualify these Linux-specific real-data adapters.

```sh
cargo fetch --locked
python3 tools/prepare-prose-tools.py --include-sources
# After initial public build-input downloads:
python3 tools/prepare-prose-tools.py --offline
bash tools/test-m7b.sh
```

The provisioner verifies all 49 exact package artifacts and two deterministic
Git-source repacks. Schema-2 installed metadata is derived from wheel bytes,
including retained vendored metadata. Verification precedes publication. An
unknown or corrupt existing runtime is preserved and refused; choose a new
output directory for a rebuild rather than editing its marker to bypass checks.
`--output` uses that output's sibling `wheels` cache when validating a runtime.
The relocatable payload uses Debian's `/usr/bin/python3`, not a bundled or
arbitrary host interpreter. Retained notices and model declarations remain in
`docs/python-tools-lock.json`, `docs/python-tool-notices.json`,
`docs/prose-model-licenses.json` and `docs/lexicon-licenses.json`.

For a custom Cargo output, export `CARGO_TARGET_DIR` and/or `GRIMOIRE_BINARY`.
For a separately provisioned runtime, set `WRITINGDECK_TOOLS_PYTHON` to its
`runtime/bin/python3`. The package auditor also requires the matching wheel
cache. Python tests fail if required genuine integrations are unavailable;
root/missing-binary skips cannot count as passing qualification.

`tools/measure-prose-tools.py --output NEW-RESULT.json` measures two synthetic
fixtures through the actual CLI, with two fresh processes per engine. Its
second run is not a persistent warm-model benchmark. Measurements include
CPU/wall time and Python worker peak RSS; they exclude physical battery claims.

## Remaining acceptance boundaries

Fresh full Rust configurations, Clippy, real Python/runtime/terminal tests,
retained-notice audits, software benchmarks and a new networkless boot image
must align with the tested source. The image now stages the verified Python
runtime and notices and gates actual Wi-Fi firmware payloads. Its installed-tool
guest probe passed in one fresh boot, with successful native/proselint/WordNet
and Harper checks and the documented Nabokov CPU-denial outcome. Earlier failed
attempts remain in local diagnostic logs; no application resource limit was
raised. These checks qualify M7b software and its stated failure boundaries.

The production installer/authentication, input-safe lock, signed update/recovery,
M8 continuity/annotations/variants/read-through, M9 footnotes/backup, complete
tutorial, independent corpus review and physical qualification remain tracked
in issues #3–#14. None is implied by passing M7b software tests.
