# Offline Story Spark

Story Spark selects from 1,047 original plot situations and 84 original creative exercises. No model, server, network connection or manuscript analysis is involved at runtime. The content was drafted with AI assistance and agent editorial review, not independent human or legal attestation. Original wording is MIT to the extent rights subsist. Historical Plotto/OCR and modern prompt lists are not bundled or counted.

## Modes and controls

Open **Writing tools → Productivity tools → Story Spark**, use the palette, or select its dashboard widget. Opening is read-only: nothing is generated until you choose.

- **1** plot only; **2** exercise only; **3** compatible combination; **4** fixed daily combination.
- **p** rerolls plot while preserving the exact exercise. **e** preserves plot and rerolls exercise. **r** rerolls both. If no compatible different component exists, the old result remains and an explanation is shown.
- **f** favorites the exact result; **x** dismisses that exact pairing. Components may still occur in different pairings.
- **h** history; **F** favorites; **Enter** recalls a selected reference, never resamples it.
- **o** draft filters: category, genre, starting difficulty and suggested duration. Arrows adjust; Space toggles a genre; Enter saves; Esc cancels. Empty genre/category selection means all. Existing daily results and kept reroll components are unchanged.
- **v** provenance; **t** today's optional challenge; **u** explicitly reloads preserved state.
- **PgUp/PgDn** or arrows scroll long results. **Esc** returns without inserting anything.

Recent component IDs are avoided across twelve results. If that exhausts the pool, a receipt explicitly reports recent-repeat relaxation. Genre/category/difficulty/duration filters and dismissals are never relaxed. A requested reroll must actually change its component. Existing-text revision/read-through exercises are independently available in mode 2, not combined with a fresh plot requiring text that does not exist yet.

## Daily and recovery

Daily needs a verified IANA timezone. Its local civil date, original zone, corpus version, filters, seed and component IDs are frozen. Reopening after reboot, DST changes or travel on the same date restores that result. Rerolling it creates a separate exploration; it does not replace the day. A newer corpus never silently regenerates old daily results or favorites; unavailable historical components are reported while references survive.

Metadata contains references, settings and receipts, not copied manuscript prose. Unknown/corrupt/future/conflicting state is preserved; it is not reset over favorites. History retains 128 results, favorites 64, dismissals 128 and export receipts 256. Fixed daily/assignment files are separate; no indefinite archive or power-cut transaction is claimed.

## Explicit result actions

**s** saves a Markdown note; **a** opens the actions menu.

- Save once, reuse the same note on retry, preserve later edits; missing saved originals are not recreated from metadata.
- Pin that same saved source, not a copied card body.
- Capture in the real scratchpad, or as a Markdown task/outline note. These notes are not M8 structured story facts or a fake task database.
- Create a new **empty** experimental manuscript scene with a reference to the saved prompt. It is excluded from compile; the active original is never replaced.
- Start a real suggested-duration timed sprint carrying the source reference. Timer completion does not prove creative completion and does not change the writing mode automatically.
- Assign today's optional challenge, then **a** accepts and **x** explicitly self-reports completion. This is separate from M5 measured criteria. Daily challenges also has **S** to open this optional Spark assignment.

Source export reserves a prepared receipt before exclusive creation. Failed publication leaves a recoverable original and retries the same target; conflicting text is never overwritten. This is not a filesystem lock or a cross-file power-loss transaction.

## Static and optional roulette

Static accessible output is the default. **l** enables a brief presentation-only indicator after selection. Selection and history are already fixed; animation never draws random values or changes content. Focus and low-power modes always suppress it. No animation is needed to read, recall or use a result.
