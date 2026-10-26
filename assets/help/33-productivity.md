# Recorded edits, goals and daily challenges

Open **Productivity ledger**, **Advanced goals**, **Calendar and rest days**, **Advanced sprints**, **Milestone collection** or **Daily challenges** from Ctrl-K or Writing tools. The original daily/book/scene goals and basic sprint controls remain available separately.

## Recorded, not inferred

The local ledger observes actual text replacements in both source editors. Keyboard input, deliberate paste, find/spelling replacements, formatting, undo/redo and explicit history restoration have separate origins. Programmatic loads and external-file adoption are not authored input. Ordinary Markdown stays authoritative; no manuscript prose is copied into these JSON records.

Word policy v1 counts whitespace-delimited source units containing alphanumeric characters, excluding private notes/TKs and Markdown footnote labels. It is **not** the native Grimoire whitespace counter or a full rendered-Markdown parser: code and link destinations can count. A surviving character preserves a unit's identity while you grow a word; full selection replacement removes and adds actual units, even at equal final word count. Formatting markers do not re-author the inner prose. Footnote additions/deletions and reclassification remain separate; the optional footnote switch applies to ledger goals, not native source counts.

Gross input additions/deletions exclude undo, redo and historical restoration. Recorded editor net includes their signed changes and footnote transfers, but not file creation/import/promotion/reordering or external changes. Paste is included in input-addition goals and excluded from keyboard-only goals. A change of source or mode never invents an interval of writing.

Activity credits only a pair of observed edits in the same document/mode within the idle cutoff. No startup, idle tail, offline or reboot time is added. Keyboard-pair time is labelled as such, **not proven exclusive typing time**. Monotonic duration cannot be converted into a particular civil day after an ambiguous wall-clock/timezone change; those milliseconds are unassigned. DST uses IANA civil dates when available; the actual fixed UTC offset is always recorded.

The v1 whole-source measurement bound is 262,144 bytes per mutation. Above it, words are explicitly unmeasured, not zero; known character/operation counts remain. Do not interpret this bounded implementation as incremental parsing, low-power qualification or a unified publishing source map.

## Controls

| Where | Controls |
| --- | --- |
| Every page | 1–6 pages; Tab next; arrows choose; PgUp/PgDn scroll; r explicitly reloads/retries metadata; Esc back; F1 this policy |
| Edits | t tracking; m Drafting/Revision/ReadThrough; f footnotes; i idle seconds; a optional milestones; n quiet notices; d challenges |
| Goals | n metric/period preset, then edit its numeric target; Space enabled/disabled; Delete removes only selected goal metadata |
| Calendar | n offline November 50k preset; Space toggles selected rest date; b/e start/end ISO dates; g target |
| Sprints | n variant/target; p pause/resume; x stop; b optional timed break; k skip a break while retaining its record |
| Collection | a optional milestone enable/disable; n quiet notices; earlier awards are retained |
| Challenges | a accept; c check a measured criterion; x explicitly self-report a subjective exercise; R explicitly reroll; f future filters; left/right dates |

Pasted commands in these read-only pages are ignored; they cannot accept/reroll tasks, toggle preferences or change hidden prose. Target/date fields accept only their bounded numeric/ISO input. Escape cancels field input. All source undo, selection, save, conflict and recovery safeguards remain in place.

## Goals and calendar

Advanced goals cover daily, Monday–Sunday weekly, monthly, current native novel/scene totals, observed active minutes, Revision-mode operations, footnote additions and configured deadline windows. Native source totals retain Grimoire's own word policy; they are not retroactive gross or active-time evidence.

The offline November preset targets 50,000 **recorded editor net words** over November 1–30. It is not affiliation, endorsement or certification of new exclusive typing. Start/end, target and rest days are explicit choices. Pace includes today if it is a working date; ahead/behind compares the end-of-day plan. Forecast requires at least three recorded work dates and assumes the mean of the last at most seven recorded dates repeats, including cuts. Missing days are not silently zero, forecasts are estimates, and no workdays left yields an unavailable pace rather than division by zero. Earlier calendar configurations are retained when a new preset replaces one.

## Sprints and milestones

Time, net-word, revision, no-editing and measured-challenge-linked sprints use monotonic running time. Pause excludes later edits until explicit resume. Optional breaks pause the work sprint and have their own retained elapsed record; finishing a break does not force work to resume. Restart shows the last checkpoint paused: wall-clock absence is never added. A no-editing experiment **never discards a deletion**; the deletion is retained and the restriction is reported as broken/uncertain.

Milestones are optional, deduplicated and non-punitive. Keyboard additions and revisions are measured quantities, not quality judgments. Finished-source statuses are author assertions; learning and subjective exercises are explicit actions/self-reports. Disabling retains the collection and awards nothing new. Notifications default to quiet and never interrupt focus writing.

## Daily challenges and persistence

The bundled bank contains 30 original MIT exercises across all ten categories. It is a small usable exercise bank, separate from M6 Story Spark’s 1,047 agent-accepted original plots and 84 exercises; see Story Spark help for its original-content review limits.

Assignments persist by local civil date with creation timezone, stable record references, source stamps, seed, bank version, filters and lifecycle history. A reboot does not reroll; a repeated date retrieves its existing assignment, including after timezone travel or DST. The initial set prefers recent-unseen exercises; explicit rerolls refuse a hidden repeat when their eligible pool is exhausted. Filters do not silently replace today's tasks. No assignment is fabricated for a past missing date.

Accept a measured task before working, use its stated Drafting/Revision mode, then checkpoint real edits with Ctrl-S and press c. Work before acceptance is excluded. Subjective creative/reading tasks use x and stay **SelfReported**, never automatic prose/reading verification. Disabling challenges generates no new assignments and sends no challenge notice.

Metadata uses bounded, checked, guarded files under `.grimoire/writingdeck/`. Future/unknown/corrupt/conflicting files are preserved, not overwritten as defaults. Pending operation groups remain in memory on write failure. Normal quit/book switch requests a checkpoint; explicit force quit can lose uncheckpointed metrics or timer intervals, never an excuse to claim they were recorded. This is controlled failure handling, not power-loss, locking/CAS or hostile-filesystem certification.


Capital S on Daily challenges opens the separate optional Story Spark assignment. Its completion is explicitly self-reported, not a measured M5 criterion. Timed sprint targets are minutes; Spark timer completion does not prove creative completion.
