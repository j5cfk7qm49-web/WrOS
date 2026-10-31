# Shared story identity safety

The shared document registry now refuses missing or incomplete identities when
existing continuity facts/events, milestone snapshots or scene variants still
reference them. A real compiled-app reproduction showed the previous failure:
after retaining the original registry as a fixture backup, a fresh registry
assigned document ID 1 to a different scene and restored the first scene's saved
text into it. Original source, metadata and backups are generated fixtures here;
no real manuscript was used.

Loading a registry checks the document references in the bounded current story
stores. A guarded metadata update checks the existing context before invoking
its callback and the candidate context before publishing. This stops a cached
empty registry from allocating an ID that newly arrived saved story data already
uses. Unknown/future/corrupt, oversized or aliased dependency data is refused and
preserved. Book-wide facts without document references remain valid with a fresh
registry. Owned tree moves perform the same check before moving source.
An interrupted move must be explicitly reconnected before any new document
identity is allocated; the moved source cannot silently gain a competing ID.

Already registered source IDs use a verified read without rewriting the registry
on every lookup. The writer's existing source ownership/backing checks, guarded
saves, history and Undo remain in use. No title, hash, filename or current-tree
index is used to reconstruct the missing mapping. Restoring the original registry
from a known matching project backup makes the existing references usable again.

This adds context checks to the existing guarded store, not a new identity or
transaction engine. It does not supply a recovery UI for an unprovable mapping,
authenticate an arbitrarily edited registry, or make non-cooperative cross-file
writes atomic. Existing external/interrupted-move reconnection requirements and
plain dated scene history remain separate. Original Markdown/YAML, saved story
data and the unavailable registry are never rewritten as a guessed repair.

Focused core checks cover milestones, variants, fact/event links, empty/missing
registries, cached allocation, book-wide facts and malformed/future/oversized
dependencies. Actual app checks at 80x24, 100x30, 120x40 and 160x50 prove refusal
before comparison/restore, unchanged scenes/sidecars, available ordinary writing,
save/Undo/help, then correct-target restoration and Undo after an explicit
original-registry recovery. The [independent full Debian run](evidence/story-identity-local.json) passed
1077 Rust tests per configuration, 185 Python tests with zero skips, both Clippys and 21 gates. [Provenance and frames](evidence/story-identity-local-provenance.json) bind its source and binary.
