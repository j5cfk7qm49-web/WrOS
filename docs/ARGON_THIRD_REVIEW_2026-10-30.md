# Argon third submission review

The C7445FC3 submission uses current `development` at `c502466` as its base.
All ten patch outputs match the ZIP exactly. The archive has 1180 files and ten
differences against that base; no qualified file absent from the ZIP was removed.
Original inputs and detailed probes are retained privately outside the repository.

- ZIP SHA-256: `70adc16404726caea6bf42b4f2332b9cf5f12ec0557f997284f23f1c56c81bb5`
- Patch SHA-256: `58933b20f0a2e3a02e30cf7b61e6355d745b17264bc0c6d4eafce4fefab74446`

This revision preserves PRs 63/64, shared document identity, schema-2
`scene_snapshots`, source-before-typography handling and genuine Word/PDF note
paragraphs. The read-only submitted workspace passes ordinary-builder Debian
`cargo check --workspace --no-default-features --locked --offline -j2`.

The new milestone regression does not compile: seven diagnostics concern
nonexistent `reconcile_project`, `document_id` and `save` methods, plus an
`unwrap_err` Debug requirement. The integration adapts that test to the existing
`ReviewNotes::register` and guarded `Store::update` APIs, verifies an actual
project index shift, reloads schema-2 data and checks stable identities. Argon's
additional Markdown/code/URL/Word-paragraph regression is retained as well.
No store internals or unsafe identity migration were added to satisfy the test.

The four submitted Python helper tests pass using disposable files, real GLib
and a synthetic signing key. The punctuation regression now exercises maintained
GKeyFile serialization/readback. Restic command planning and GPG/hash-chain
checks are useful, but are not a complete consistent backup/restore or update
workflow. The unused sleep-readiness helpers still cannot establish secure
lock-before-suspend. These helpers are preserved in the contribution archive;
ordinary product operations remain guarded while writing workflows take priority.

The next implemented writing increment builds [native EPUB notes](EPUB_FOOTNOTES.md)
on the preserved current parser, editor and export machinery. Submission tests,
focused checks and full software qualification establish different scopes;
neither constitutes full writing-environment or OS completion.
