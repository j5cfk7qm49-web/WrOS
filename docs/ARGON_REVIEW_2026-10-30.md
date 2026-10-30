# Argon contribution review (2026-10-30)

The supplied ZIP and unified patch describe the same 41 changed files against
`4039600138c3b4405a4220bdd9fd098035cd4e95`. Both originals were retained outside
the repository before applying the patch. ZIP SHA-256:
`1850b770afaeff840cd4671e03ff8bd4eb6741ed285e9df99f6c12b857881984`;
patch SHA-256:
`07706bd08bca2ff1aa8b40b7e2b3b85305ce73b38d33f36a65cf9bc66429396e`.
The contribution is useful unfinished work, rather than completed M8/M9 or OS
qualification. Its original completion claims were not adopted as evidence.

## Writing integration

- Continuity facts, timeline events and threads use shared shell screens and the
  guarded statefile store. New fact/event links use the existing durable document
  registry, not project-tree indices. Schema-1 prototypes are preserved and
  refused pending explicit migration. Conflict enumeration refuses more than
  512 results rather than returning a partial clean-looking report.
- Milestones and scene variants use the same document registry. Comparison
  reuses the existing history diff and its presentation. Restore requires the
  same reviewed editor revision, unchanged metadata and backing source, and
  successful recovery-variant and history writes. `r` explicitly restores;
  Enter only opens a comparison. Normal editor Undo and YAML ownership remain
  in effect. Store size limits can refuse additional snapshots without pruning.
- Chapter read-through reuses the existing voice engine pipeline. It stays in
  the current chapter and binds queued paths to source digests. Marking a
  sentence uses guarded anchored revision notes and reports persistence errors.
- The `wros` CLI alias retains `writingdeck` compatibility. `docs-audit` compares
  two explicitly supplied local files with bounded reads. It does not enroll a
  provider, transmit a manuscript or certify Google Docs interoperability.

## Prototypes that did not qualify for integration

The original archive retains these parts for later work:

| Submitted area | Finding and disposition |
|---|---|
| OOXML/EPUB builders | Invalid Rust string literals; no document references, relationships, styles or rendered page integration. Existing rich-footnote refusal retained. Lesson 34 remains unavailable. |
| Custom Rust/Python backup bundles | Two competing new backup formats; unbounded reads/traversal, insufficient parent-symlink and destination handling, and no consistent project snapshot. Not exposed as a verified backup/restore workflow. Reuse Restic for the real integration. |
| Signed offline updates | The helper accepted a file named `InRelease` without verifying its signature or binding package hashes. Not installed or exposed. |
| Wi-Fi keyfile enrollment | Unconnected privileged helper with hand-written keyfile escaping and incomplete secret/profile lifecycle. Existing read-only NetworkManager integration retained; libnm integration remains required. |
| Suspend readiness | Unused command/bool helpers did not establish authenticated lock-before-sleep or resume ownership. Qualified manual lock path retained. |
| Tutorial completion | Creating another book, viewing a footnote dock or resolving a local conflict does not establish page-footnote output, mobile sync or provider enrollment. Corresponding completion claims removed. |

The optional wiki-link checker remains a bounded model; export selection review
does not claim full link/asset validation. Maintained Markdown parsing excludes
code and private material from its inspection.

## Verification

Focused writing and full Debian qualification are in progress. All fixture
projects and HOME directories are generated; no real manuscript, provider,
credential, disk, firmware or suspend operation is used for these tests.
