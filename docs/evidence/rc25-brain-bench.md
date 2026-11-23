# RC25 Brain storage qualification

The benchmark uses Loro 1.18.0 and SQLite FTS5 via rusqlite 0.43.0. Loro merges
the native tree and the Brain operation history. The Brain reference projection
applies the rules for local Undo, deletion and recovery. The storage tests
verify that an ordinary Loro snapshot and its pending sidecar reopen together,
and that the journal keeps the original pending bytes. [Loro persistence](https://www.loro.dev/docs/tutorial/persistence) · [SQLite FTS5](https://www.sqlite.org/fts5.html)

## Workload and limits

Use seed 25001 for the first recorded aged run. It generates 50,000 visible
nodes, 10,000 deleted nodes, 500,000 edits and 100,000 simulated review events.
Notes range from short lines to 2 KiB. The runner builds a real SQLite FTS5
index and Markdown copies. A worker refreshes both during measurement. Each
latency percentile uses 20 samples with the nearest-rank method.

| Measurement | Limit |
| --- | ---: |
| Cold store open and first note, p95 | 3,000 ms |
| Durable edit and text preparation, p95 | 50 ms |
| Bounded FTS search, p95 | 150 ms |
| Peak process RSS during checkpoint | 512 MiB |
| Journal bytes per edit, amortized | 64 KiB |
| Temporary store growth during checkpoint | One checkpoint plus 64 MiB |

An open loads a new document and builds the first projection within the same
benchmark process. The operating system may cache file data between opens.
The edit measurement covers durable save and line construction. Measure actual
keyboard-to-display latency when the desk connects to the store. The benchmark
samples RSS, temporary store size and index WAL bytes every five milliseconds
during the checkpoint. It records Linux process writes during the edit samples
separately, including writes from the background worker.

## Running on the HP Stream

Build with `nix build -L .#brain-bench`. Copy `result/bin/brain-bench` to the
HP Stream running WrOS with the same pinned runtime. Run it from a directory on
the eMMC so the synthetic scratch store uses the same disk as the notes.

~~~sh
./brain-bench --seed 25001 --profile aged --revision CANDIDATE_COMMIT \
  --output "$HOME/rc25-brain-bench.json"
~~~

Use the source commit in place of `CANDIDATE_COMMIT`. Keep the output for each
run under a different name. The `smoke` profile checks the runner with a small
corpus. Use the `aged` run on the laptop for the Stage 0 decision.

## Results

HP Stream measurements are pending.
