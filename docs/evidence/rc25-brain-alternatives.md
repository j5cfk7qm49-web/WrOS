# RC25 storage alternatives for the HP Stream

The original Loro and Automerge candidates loaded and replayed the entire
operation history to display the first note. On the HP Stream, both also
spent about 230 ms ranking full-text matches before returning 20 results.
[Original measurements](rc25-brain-bench.md#hp-stream-11)

Logseq's DB graph stores its data in SQLite and treats pages and blocks as
queryable nodes. The prototypes borrow those design principles. A stable
node ID addresses the current text directly, and the application queries an
indexed store for the next view it needs. WrOS keeps its own causal model
and readable Markdown copy. [Logseq DB graph guide](https://github.com/logseq/docs/blob/master/db-version.md) · [Logseq node changes](https://github.com/logseq/docs/blob/master/db-version-changes.md)

## Paged Loro

Loro keeps the replicated operation history and its native movable tree. A
Lamport clock routes an edit into a map covering 4,096 clock positions.
Loro changes that depend on missing data wait across a restart, including
changes that cross map boundaries. The checksummed journal retains the
active and previous checkpoints.

A separate SQLite projection stores visible bodies and the identity of a
single causal text winner when available. Persist the Loro update first.
An ordered Loro version vector identifies the projection's contents on
open and after edits. A crash between updating the two files leaves the
projection stale, and the next read rebuilds it from the verified history.
The projection uses SQLite WAL with `synchronous=NORMAL`. The checked Loro
journal provides durability for acknowledged edits. A power loss can roll
back the view to an older version, which the stamp detects on reopen.
[SQLite synchronous modes](https://www.sqlite.org/pragma.html#pragma_synchronous)

## SQLite ledger

SQLite stores immutable operations under their logical IDs, together with
review events and the current projection. The same transaction makes an
edit and its view visible. A received action is idempotent, and an action
with missing dependencies stays in the ledger until it can be projected.
The common model resolves the operation graph, including moves, deletes,
conflicting fields, splits, joins and Undo.
The operation and view tables keep text in SQLite table leaves and index
logical IDs separately. This gives typical note bodies more room per row.
[SQLite table and index pages](https://www.sqlite.org/fileformat2.html#b_tree_pages)

SQLite's WAL handles interrupted saves on the live database. The prototype
also takes checksummed backups using SQLite's page-copy API. A verified
older backup opens in a separate directory with a fresh device identity
and can rejoin by importing later changes. Recovering later edits after
physical database damage depends on having copies of those changes.

## Foreground behavior and test scope

Both prototypes can apply a text replacement directly when the whole visible
body has one token with one causal winner and the new edit depends on that
winner. More complex changes, imports and invalid projections fall back to
the reference replay. That path still reads all history and needs work
before it can serve a large graph interactively.

Search matches across the full FTS index using SQLite's phrase and Boolean
predicates. It takes the first 128 matching rows in stable index order and
ranks that group to return 20. Global relevance ranking would require a
different latency budget or a different retrieval implementation.

The benchmark keeps the original fixture, background FTS and Markdown work,
and thresholds. Each cold worker reopens an already built projection and
reads the first note. The edit sample includes durable save, reading the
updated note from that projection and formatting the line. Keyboard-to-
display timing belongs to the installed app once the UI uses these stores.
The write metric counts the journal and applicable projection WAL, or the
ledger WAL. The separate foreground I/O measurement uses Linux per-thread
write counters and excludes the background FTS and Markdown writer.
Checkpoint measurements cover publication of real backup files.

## Reproducing the comparison

```sh
nix build -L .#brain-bench
./result/bin/brain-bench --engine loro-paged --profile aged --seed 25001 \
  --revision CANDIDATE_COMMIT --output "$HOME/loro-paged-aged.json"
./result/bin/brain-bench --engine sqlite-ledger --profile aged --seed 25001 \
  --revision CANDIDATE_COMMIT --output "$HOME/sqlite-ledger-aged.json"
```

Copy the executable to the HP Stream and run the two commands there, using
the executable's actual path and the commit used to build it.

## HP Stream results

The final comparison ran commit f93d346 on the HP Stream 11-ah0XX with
WrOS RC24. The Celeron N4000 and internal encrypted eMMC ran both smoke
and aged profiles sequentially. The aged workload retained all 500,000
operations, 50,000 visible nodes, 10,000 deleted nodes and 100,000 review
events. Each latency result uses 20 samples.

| Measurement | Limit | Paged Loro | SQLite ledger |
| --- | ---: | ---: | ---: |
| Cold open and first note, p95 | 3,000 ms | 413 ms | 6.2 ms |
| Durable edit through text preparation, p95 | 50 ms | 45.4 ms | 27.8 ms |
| Foreground FTS, p95 | 150 ms | 43.2 ms | 47.0 ms |
| Peak RSS during checkpoint | 512 MiB | 158 MiB | 28 MiB |
| Journal and projection WAL per edit | 64 KiB | 10.1 KiB | 28.8 KiB |
| Temporary store growth at checkpoint | Checkpoint + 64 MiB | 51.8 MiB | 396.1 MiB |
| Linux-attributed foreground writes per edit | 64 KiB | 12.4 KiB | 32.8 KiB |

Both prototypes met the six benchmark limits and stayed under 64 KiB on the
separate foreground write counter. Checkpoint files were 51.8 MiB and
396.1 MiB respectively. The SQLite layout change reduced its checkpoint from
about 713 MiB on the first run. Including the active store and two retained
generations, storage started at 173 MiB for paged Loro and 1,188 MiB for
SQLite, plus the shared FTS index and Markdown copies.
[Paged Loro raw results](rc25-brain-hp-stream-loro-paged.json) · [SQLite ledger raw results](rc25-brain-hp-stream-sqlite-ledger.json)

All 26 focused qualification tests and 16 default-feature tests passed.
Each candidate converged against the reference model in 16 two- and
three-device schedules. The tests also cover a stale Loro projection after
a saved update, updates that cross a Loro map boundary, SQLite recovery
after a torn WAL, and an older verified ledger backup that rejoins with a
fresh identity. Workspace formatting, strict Brain Clippy and the Nix
benchmark build passed.
[Test commands](../TESTING.md#rc25-brain-storage)

### Paged Loro runtime retest

The HP Stream reran the aged and smoke workloads on commit `03b4d06` after
paged Loro became the runtime store, with the indexed outline and the local
causal frontier persisted in SQLite. Both profiles met their limits on the
same encrypted eMMC. The aged run measured 413 ms p95 to open and read the
first note, 43.9 ms p95 from durable edit to text preparation, and 43.1 ms p95
for FTS. Peak RSS during checkpoint was 159 MiB and journal plus projection
WAL averaged 14.1 KiB per edit. The installed desk's input-to-display timing
still needs its own pass.
[Aged run](rc25-brain-hp-stream-loro-paged-runtime.json)
