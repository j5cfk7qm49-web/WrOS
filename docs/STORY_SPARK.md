# Offline Story Spark

**M6 software implementation, not production/human/legal qualification.** The actual core, cached TUI reader and explicit actions are implemented. Targeted core/UI/compiled-PTY tests exist; the source-aligned full run is recorded separately in [STATUS](STATUS.md) and [test-results.json](evidence/test-results.json). The original content and its review limits are in [CHALLENGE_LIBRARY](CHALLENGE_LIBRARY.md). There is no runtime AI, server, manuscript analysis or automatic insertion.

## Open and use

Choose **Writing tools → Productivity tools → Story Spark**, the palette's **Story Spark**, its additive dashboard widget, or an explicitly configured logical alias. No default key was rebound. Merely opening the reader generates nothing.

- **1** plot-only; **2** independently usable exercise-only; **3** compatible combination; **4** fixed daily combination.
- **p** replaces only plot, **e** only exercise, **r** both. Requested components really must change. The kept component remains exact even after changing future filters; compatibility is checked again. Empty pools report an explanation and retain the previous result.
- **f** favorites/unfavorites the exact reference; **x** dismisses the exact pairing, not every possible use of both components.
- **h** history; **F** favorites; arrows and Enter recall the selected reference without resampling. Older unavailable corpus components remain visible as retained references, not silently regenerated content.
- **o** edits category, genre, maximum starting difficulty and suggested duration. Enter saves explicitly; Escape cancels. Empty category/genre selections mean all. Difficulty is an editorial starting-burden estimate, not a quality ranking.
- **s** saves the result note; **a** opens real source actions; **v** provenance; **t** optional assignment; **u** explicit reload. PgUp/PgDn scroll long content. F1 opens embedded offline help.

Existing-text revision/read-through exercises remain useful in mode 2. They are excluded from fresh combined situations rather than pretending a draft already exists. Explicit narrative conditions and exclusions reject incompatible form/time/dialogue/person/sensory combinations; mere presence of an event or a historical setting is not invented as a hard constraint.

## Deterministic state and date policy

`spark/state.json` under `.grimoire/writingdeck/` stores schema/version, persisted SplitMix64 seed/counter, selected component IDs/hashes, filters, history, favorites, dismissals, recent references, source receipts and the roulette setting. No manuscript or generated prompt body is stored in JSON. This PRNG is noncryptographic. The frozen reference records corpus version, seed/counter interval, local date/zone and a pool-context SHA-256. That digest identifies selection context; it is not authentication or a standalone resampling snapshot.

Daily is separately stored at `spark/daily/YYYY-MM-DD.json`. It requires a verified IANA civil timezone. The first successful date selection freezes its original zone, filters, version and components. Same-date reboot/DST/travel reopens that result. Reroll creates an ordinary combined descendant and does not overwrite the day. Daily selection does not advance the main stream counter. A corpus upgrade does not substitute current wording into a historical hash/version: unavailable content is reported while references remain.

Recent component IDs from twelve results are avoided. If only that policy exhausts a pool, a receipt explicitly marks recent-repeat relaxation. Hard filters, compatibility and dismissals are not relaxed. History retains 128 results, favorites 64, dismissals 128, recent context 12 and source receipts 256. Favorite/dismissal/source capacity failures are explicit; no silent favorite/source eviction or automated receipt pruning is claimed. Daily and assignment date files are separate, not an indefinite bounded archive.

## Source-safe explicit actions

| Action | Actual behavior / boundary |
|---|---|
| Save | Exclusively creates `notes/sparks/{result}.md` with the prompt and full provenance. Retries reuse that original, preserve later edits and never recreate a missing ready source |
| Favorite / dismiss | Guarded reference-only state changes for the exact selected result |
| Pin | Pins that same saved Markdown source. Repeating this receipt-driven action selects the existing active card and retains its independent scroll; generic pinboard duplicate-card behavior is unchanged |
| Scratchpad | Exclusively creates a real scratch original, adopts only the exact intended source fingerprint into the workflow index, then opens its actual primary editor. First adoption checks next ID/capacity and external metadata; later registered edits survive |
| Outline / task | Creates source-backed Markdown notes, including a real checkbox for task capture. This is not an invented M8 structured story/task service |
| Experimental scene | Saves the prompt note once, then creates an **initially empty**, independent manuscript scene. Frontmatter references the note and IDs, with `compile: false`. Original active prose is never replaced. Retry opens the existing independent scene without erasing later writing |
| Sprint | Atomically publishes a real M5 timed sprint and optional `Sprint.spark` reference in the same settings update. Suggested duration is **minutes**. Existing sprint rules, original work mode, pause/resume and restart policy apply. Timer completion does not complete creative work |
| Optional daily challenge | Assign → explicit accept → explicit **self-reported** completion. Separate from the thirty-exercise M5 measured bank; Spark IDs are never injected into that bank. Daily challenges also has **S** to reach this optional assignment. A date change requires reviewing the newly displayed assignment before accepting/completing |

Exports reserve a guarded **prepared** receipt, exclusively create the intended target, then mark it **ready**. A partial publication failure retains the original and retries the same target; conflicting prepared text is not overwritten or copied elsewhere. A ready source can retain writer edits. Markdown remains authoritative; this is not a live resource-link rebase, competing body in JSON, cross-file power-cut transaction or hostile-race CAS/lock.

Unknown/corrupt/future/conflicting metadata is preserved and displayed. Selection/publication is refused rather than resetting favorites. Explicit reload of missing/corrupt already-loaded metadata retains recoverable cached references; restoration is required before publication. Errors and refused normal quits are visible above the modal, not hidden only in an internal message.

## Static display and tutorials

Static, scrollable output is the default at 80×24, 100×30, 120×40 and 160×50. **l** enables a short presentation-only roulette indicator on selection. Result/seed/history are already fixed; frames never draw random values or change content. Focus and low-power suppress it; it does not require a faster wake rate. This is not a physical battery/latency qualification.

Safe-demo lesson **22** requires plot-only result publication to a real saved note. **23** requires a successfully persisted exercise-only favorite. **24** checks the stored ancestor chain for independent plot/exercise and joint rerolls, a retained favorite, and newly ready independent scene creation/opening. Generation, navigation, an existing ready-source retry, or a fake completion button cannot substitute for the required save/scene transition. These events do not complete writing exercises in an ordinary novel.

## Verification and remaining gates

- `cargo test -p grimoire-core spark:: --locked --offline -j2`: 28 dedicated core cases, including malformed corpus, exact provenance, deterministic fixtures, mode/compatibility checks, rerolls, hard/recent exhaustion, history/favorite limits, daily/DST/travel/upgrade behavior, source conflicts/retries and optional assignments.
- Three workflow adoption cases cover exact first fingerprint, later edits, ID ownership and external future metadata.
- `cargo test -p grimoire-tui --no-default-features spark_ui_tests --locked --offline -j2`: 28 actual controller/UI cases, including source originals, all sizes, real actions, tutorial noncompletion, injected date rollover, presentation-only frames and active-sprint failure/normal-quit/restart.
- `PYTHONPATH=os/tests python3 -m unittest test_spark_corpus test_spark_pty -v`: six build/provenance contracts and four actual non-root compiled-app PTY methods. The latter include four-size separate-process restart/edit/undo, paste guards, fixed daily/favorite recall, visible corrupt/future-state and prepared export recovery, independent scene editing, and real active-sprint checkpoint refusal/restart.

The retained `spark-pty-{size}.txt` files are completed differential-terminal frames, not synthetic illustrations. The shim is not a full terminal emulator. Agent authoring/editorial review and duplicate checks do not prove independent human/legal clearance or every reader's assessment of substantive distinctness. Physical keyboard, power loss, low-power hardware, signed distribution, archival and production security gates remain open. M6 is not completion of M2b or M7–M10.
