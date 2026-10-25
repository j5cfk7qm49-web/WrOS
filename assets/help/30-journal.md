# Writing journal

Open Writing journal in Ctrl-K or Writing tools, or press j on the dashboard. The journal separates read-only recorded facts from an ordinary Markdown reflection. It never writes prose on your behalf.

## Recorded facts

Totals are the current Project cache at a clean save boundary, not a filesystem-wide atomic snapshot. External/structural changes can affect them. A fully clean, successful manuscript save records the first checkpoint in this app/book context, then at most one per ten minutes. Normal quit/book-change requests record a final saved snapshot. Known non-UTF-8/unreadable/unsettled manuscript sources inhibit complete-total observations rather than becoming invented zero counts. If the initial book total was unavailable, its since-open delta and daily net remain unavailable for that context even after repair; current complete totals can then be recorded. These are sampled observations, not every edit or proof the process exited. Opening a page or saving notes is not a manuscript-writing event.

Each fact preserves its observed time and local offset, app-open time, saved manuscript total, existing daily net count when its tracked date matches, and the manuscript delta since app open. That last delta can include external/structural changes: it is not exclusively words you typed. Gross additions/deletions, active/idle duration and drafting/revision classification are not measured here. Local facts/history are not backup.

## Reflection source

Tab switches between facts and the source preview. Enter explicitly creates/opens notes/journal/YYYY-MM-DD.md in Secondary; e uses Primary. An already-owned source focuses its original editor. Both editors retain their save, history, undo, ownership, unavailable/conflict and recovery guards. Mapped single letters are reader commands, not a prose input box. Bracketed/clipboard paste is ignored instead of being executed as commands, and cut/undo/drag cannot mutate a hidden editor. Source bytes and raw YAML are never replaced by automatic facts.

| Key | Action |
| --- | --- |
| Left / Right | Previous / next local date |
| t | Today |
| Tab | Facts / reflection preview |
| Up / Down | Scroll this reader, not the source cursor |
| PgUp / PgDn | Reader page of ten rows |
| Home / End | Reader top / last row |
| Enter / e | Explicit original reflection in Secondary / Primary |
| c | Capture current clean saved Project counts, after a real manuscript save in this context; unchanged counts do not duplicate a fact |
| r | Explicitly reload facts and retry pending observations |
| Esc | Leave the journal |

## Preservation and limits

Schema-v1 facts live in .grimoire/writingdeck/journal/YYYY-MM-DD.json, at most 1024 facts and 1 MiB per day. Guarded append transactions never edit existing facts; unknown, future, corrupt, conflicting and failed-I/O records remain untouched. Pending observations stay in memory until explicit retry. A normal quit refuses an unsaved journal observation; a second Ctrl-Q can explicitly force exit, losing pending facts as warned. No OS lock, cross-process compare-and-swap or hostile-filesystem-race protection is claimed.

Reflections use a date-path convention, not a duplicate text store or automatic prose template. Moving one elsewhere preserves its source, but this date reader will no longer find it until its path is restored. Large-log/document CPU and power-cut durability still need qualification. Scratchpad and next-session handoff are separate original-source workflows, described in their own help topics.
