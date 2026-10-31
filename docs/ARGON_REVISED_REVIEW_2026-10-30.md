# Revised Argon contribution review (2026-10-30)

The [Argon repair prompt](ARGON_FIX_PROMPT_2026-10-30.md) provides the current
baseline, reproductions, integration constraints and required verification.

The revised submission responds to several findings from the
[first review](ARGON_REVIEW_2026-10-30.md). Its backup code now plans maintained
Restic operations, and its offline-bundle test really verifies a GPG signature
and the Release → Packages → archive hash chain. Novel/mobile tutorial events
remain unavailable, and footnote lesson credit now attempts to wait for a
matching successful PDF. These are useful corrections.

The submission still needs substantial repair before integration. Its patch is
based on `4039600138c3b4405a4220bdd9fd098035cd4e95`; it does not incorporate the
subsequently qualified shared document identity and manuscript export work.
All 42 patch outputs match the ZIP byte-for-byte. Twenty-eight files differ
from the earlier submission. Original input bytes remain preserved privately.

- ZIP SHA-256: `ad082307c381098c01ae3b0b841597c159bbc3595e2ec2399e4ef03b906f5cf7`.
- Patch SHA-256: `e3dab546a5a4de6dd12f2b4e811304d3a2d7af18c5bbfa85ac74431f12237a61`.

| Area | Independently observed blocker |
|---|---|
| Legacy migration | Inserting an earlier scene changes project indices. `MilestoneStore::migrate_v1` accepts the current index map and assigns historical scene A text to the newly inserted scene B. A saved backup preserves old bytes but does not make this identity inference correct. |
| Existing draft compatibility | The new schema-2 `document_snapshots` format rejects the currently qualified schema-2 `scene_snapshots` data. Separate path registries also do not replace the existing shared document registry and its owned move reconciliation. |
| Manuscript syntax | A genuine Markdown export changes literal code quotes/dashes and a URL containing `--`. The token adapter protects its encoded references, then still applies typography to the remaining Markdown before parsing. |
| Note structure | The OOXML footnote builder flattens two definition paragraphs into one Word paragraph. Package membership alone does not establish preserved formatting or pagination. |
| Wi-Fi values | Actual Debian GLib 2.84.4 rejects the helper's `\;` and `\#` string escapes. Its custom inverse accepts them, so the submitted round-trip test passes while the maintained parser fails. No network enrollment was performed. |
| Build and UI integration | The untouched workspace fails with six core compiler diagnostics. After three build-only repairs in a disposable copy, a workspace check finds 30 further UI/CLI diagnostics, including invalid APIs, YAML option types, error propagation and the PDF receiver borrow. |

The four independent core regressions above all fail against the disposable
copy after the minimum compilation repairs. The four submitted Python helper
tests pass, including their temporary signing-key fixture. The separate real
GLib probe fails both punctuation cases and passes the plain-name control.
These results distinguish working cryptographic verification and command plans
from completed enrollment, backup/restore, publishing and system workflows.

Keep the qualified shared IDs, source/revision guards, history, Undo and
[manuscript Word/PDF implementation](MANUSCRIPT_FOOTNOTES.md). Historical
indices require explicit evidence-based reconnection rather than automatic
mapping through today's tree. The revised Restic and signed-metadata fixtures
can inform maintained-engine integration. EPUB/paperback ideas still require
source-preservation, package and actual output checks before their guards lift.
The revised catalog has 37 non-null events; its 39-event changelog claim is
stale, and its footnote instructions do not describe the new completion action.

Only generated fixtures and ordinary Debian users were used. No real manuscript,
provider, credential, disk, firmware, account or suspend operation was touched.
The full original archives, build-only repair hashes, compiler logs, regression
sources/results and GLib receipt remain outside the repository with the private
audit tools. This review does not qualify the submitted revision as a working
OS or replace the current project's separately bound passing evidence.
