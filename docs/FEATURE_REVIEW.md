# Native environment and story-model review

The 2026-10-28 feature contribution added useful model and system-view scaffolding.
It was unfinished: three exhaustive-match errors prevented a workspace build,
and none of the new story models had editor interaction, export or speech-engine
integration. The original ten files and their tracked diff were preserved before
review. This increment continues the environment-first priority in
[the roadmap](ROADMAP.md).

## Native system views

Settings now connects the read-only network scan, saved-profile, local-account,
mounted-filesystem and cached package-plan replies to the shared Grimoire menu.
The existing background worker owns service calls; rendering and input retain
the current editor context. Explicit simulators are labelled before their first
status response as well as afterwards. Refresh retires previous results before
requesting new ones; a failed refresh cannot present an old list as current.

The Linux backend uses maintained Debian tools:

- NetworkManager's [documented terse escaping](https://manpages.debian.org/trixie/network-manager/nmcli.1.en.html)
  preserves SSID/profile colons, backslashes and significant spaces. Saved Wi-Fi
  profiles carry UUIDs. New-network credentials, connection changes and deletion
  remain unavailable pending protected enrollment and explicit target review.
- Local accounts come from bounded local account data. Missing/invalid data
  yields an error; an empty supported list stays empty. No account is invented.
- util-linux [findmnt JSON](https://manpages.debian.org/trixie/util-linux/findmnt.8.en.html)
  preserves mount paths containing spaces. Invalid/null capacity data is refused
  instead of being silently converted to zero.
- APT performs only a simulation against its existing cache. Proposed removals
  are included. The result explicitly does not establish repository freshness,
  install/download updates, schedule an offline update or qualify recovery.

The original Google backend wrote a fabricated account identity and returned
synthetic Drive documents and materialization success. Those paths now report
unavailable without modifying account files or manuscripts. Unconfigured Restic,
update installation and recovery/firmware restart requests likewise cannot invoke
their tools. The reusable typed scaffolding remains, with explicit availability.
No credential-bearing field remains in the logged/serialized system request or
shell state. There is no new password store, root grant or real-provider test.
System output above the retained limit is rejected rather than parsed partially.

## Experimental writing models

These are tested foundations under development, not completed M8/M9 workflows:

- Annotation locations return original UTF-8 byte ranges. Exact quote/context
  matching refuses ambiguous occurrences, overlapping duplicates and changed
  text. Case folding cannot generate a range into a differently sized string.
- All four new story stores reuse the existing bounded, conflict-aware metadata
  store. Schema/nested fields and identities are validated. Corrupt, future,
  externally changed, oversized and unversioned prototype data is preserved.
  No automatic migration from the prototype paths is performed.
- Continuity groups keep fact categories separate. A definite contradiction
  requires the author to declare the same single-value context on both canonical
  facts. Unscoped/changing facts only suggest review; distinct explicit contexts
  and retconned facts do not produce a definite conflict. Bounded review errors
  cannot appear as a clean partial result.
- Footnote analysis reuses the existing maintained `pulldown-cmark` parser.
  Code, escaped text, link destinations and private notes cannot supply false
  references/definitions. Multiline, missing, unused and duplicate definitions
  are covered. This is inspection, not an OOXML/PDF footnote exporter.
- Preflight follows existing private-note, TK and word-count conventions, keeps
  annotation notices on empty scenes, and refuses invalid/over-limit inputs.
  It has no compile-scope/revision binding or export hook yet.
- Sentence spans reuse pinned [`unicode-segmentation` 1.13.3](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/trait.UnicodeSegmentation.html),
  already in the dependency graph, with a narrow fiction dialogue-tag adapter.
  There is no speech engine, audio playback, multi-scene queue or highlighting UI.

Named snapshots/variants remain bounded models. They do not replace existing
manuscript history, restore drafts or implement diff/variant interaction. Stable
project-document identity, full interaction/help/tutorial work, source-revision
guards and actual publishing/speech acceptance are still required before those
capabilities can be called complete.

## Qualification boundary

Focused checks and fresh integrated Debian evidence are recorded separately from
the earlier [installer composition checkpoint](evidence/native-offline-provider-checkpoint.json).
That older image does not contain these feature changes. No further full
encrypted installation cycle or physical/provider operation is part of this
review. Boot, native enrollment, verified backup/restore, signed updates/recovery,
full M8/M9 workflows and release qualification remain open in [STATUS](STATUS.md).
