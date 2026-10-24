# Story Spark

**Planned (M6).** No mock generator is labeled implemented. Core writing remains usable without it. Corpus rights/count acceptance is in [CHALLENGE_LIBRARY](CHALLENGE_LIBRARY.md).

## Required modes

- A: plot situation only.
- B: independently usable writing challenge/constraint only.
- C: compatible plot + challenge, with independent reroll, joint reroll and stable component IDs.
- D: daily combined result persisted by local calendar date and corpus version.

All generation is local, non-AI and offline. Filtering includes categories, genre preferences and difficulty. Meaningful structural templates can supplement curated records, but trivial wording changes do not inflate the 1,000-option acceptance count.

## State and compatibility design

Persist corpus version, PRNG seed/counter, recent component IDs, history, favorites, dismissed results and daily assignments. Use deterministic fixtures; avoid recent repeats with an explicit graceful fallback when filters exhaust the pool. Changing one component should preserve the other and recheck compatibility. Contradictory constraints (e.g. dialogue-only combined with narration-only sensory description) require reroll/explanation, not an unusable instruction.

Static accessible output is the default low-power view. Optional roulette animates presentation only; it must not change reproducibility/history or distract in focus mode. No animations are necessary to use the generator.

## Result actions

Save, favorite, dismiss, independent/joint reroll; pin the same result reference; capture to scratchpad/task/outline; create a new experimental scene without touching the original; start a sprint; assign as today's optional challenge. These actions depend on the corresponding M4/M5/M8 services and must not be fake buttons.

## Acceptance / recovery / tutorial

Tests: unique substantive records and licensed provenance; deterministic modes/filters; compatible pairing; recent-repeat avoidance and exhausted pools; favorites/history persistence; daily date rollover and corpus-upgrade behavior; no manuscript mutation until an explicit action; static 80×24 rendering and focus/low-power behavior. Corrupt corpus/history is preserved/reported, not silently regenerated over favorites. Tutorial lessons 22–24 remain planned until the real engine and actions pass these tests.

## Dependency update

M3b supplies tested distinct-document Editor contexts and existing guarded saves. It does not implement the Spark corpus, roulette, filters, favorites, history or insertion/pin actions. Future actions must resolve the intended document/role and apply only on explicit writer request, with stale-source and recovery tests rather than sending generated text to whichever pane happens to be active.
