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
