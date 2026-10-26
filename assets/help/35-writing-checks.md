# Writing checks

Open **Writing checks · local grammar** from Writing tools or the command palette. It checks the active original editor, not a copied manuscript in a second editing surface. Existing spellcheck remains independent.

## Optional local model, not mandatory AI

Harper 2.13 includes a bundled part-of-speech tagger and neural chunker. This is **not a non-AI grammar engine**. There is no account, network service, model download at runtime, telemetry or manuscript transmission.

Opening the reader does not authorize a check. Press **r**, read the permission page and press **y** to run once. Permission for this once-only mode lasts only for this process, unless you previously allowed this runtime in your personal profile. No result or source prose is saved in diagnostic metadata.

Press **l**, review the scope, then **y** to enable live analysis for this novel and remember permission for this exact runtime in your personal profile. Both novel consent and a personal canonical-project-location/runtime binding are needed on a fresh launch. Importing novel metadata at a new location cannot enable live work even in a previously-consented profile. Moving a project requires re-enrollment. The reference is not authentication against profile changes or reuse of an already-authorized directory. These are two separately guarded writes, not a cross-file atomic transaction. If the second fails, personal permission may be recorded but live checks stay off without novel consent.

Press **l** while live to stop it. A failed preference write still stops this process and visibly reports whether personal-binding revocation and novel-off persistence each succeeded. **c** clears results and pauses this process without changing the saved live preference. Editing, spelling, saving and ordinary writing do not require analysis.

## Review, never automatic rewriting

- **Up/Down** selects a finding; **PgUp/PgDn** moves by ten; Home/End jumps.
- **Tab** or Right chooses an alternative.
- **Enter** selects the original source, without changing it.
- **a** explicitly applies the selected, source-safe alternative as one ordinary undoable replacement. **Ctrl-Z** in the editor undoes it. This is replacement activity, not typing credit.
- **i** ignores this finding for the current pass only, without writing a file.
- **n** mutes this rule for the novel; **G** asks before muting the rule across novels in this personal profile. Rule mutes contain engine/version/rule identifiers, not phrases or copied prose.
- **s** opens policy settings; **u** explicitly reloads settings. Broken/future/conflicting data is preserved, never reset by opening the reader.

After a policy/mute change, press **r** to check once again, or let enabled live checks run after idle. Novel and personal mutes are version-bound. A changed rule version is not silently treated as identical. Clear novel or personal mutes separately in Settings, with confirmation. Settings **k** separately asks to revoke all saved personal model permission/location bindings; this process stops, but other already-running instances need reload or exit.

## Source coordinates and safe edits

Every finding carries a document, source unit, exact content hash, request generation, configuration and engine/rule version. The editor uses **editor-body UTF-8 bytes**, with zero-based internal offsets and logical Unicode scalar columns. The reader displays one-based body line/column. This is not a claim about byte positions in the disk file's separate YAML preamble. Whole-Markdown-file adapters use an explicit different source unit.

Native Markdown is retained for Harper's parser. Private `%% notes %%`, TK markers, frontmatter in whole-file mode and excluded footnote bodies are masked without changing scalar order or line endings. Code, hidden link destinations and footnote labels are not prose. UTF-8, Unicode scalar and UTF-16 indices use checked boundary maps; invalid offsets are discarded, never rounded or guessed after stripping markup.

A replacement is refused if the source/generation/policy changed, the exclusive document lease is invalid, the backing file changed, the source is read-only/unavailable, or the replacement crosses Markdown syntax or decoded entities/escapes. Such findings can still be located for manual editing. Ordinary autosave/conflict/recovery remains the only save path; guarded checks are not hostile-race locks or an atomic filesystem transaction.

## Fiction policy and limits

Drafting favors a conservative grammar subset; Revision adds usage/punctuation; Deep review includes more of Harper's editorial rules. These are **Harper profiles**, not a claim that the later Python/readability/voice dashboard is implemented. All findings are fallible suggestions, not objective literary quality.

Quoted dialogue remains prose. Relaxed dialogue suppresses stylistic/capitalization/readability observations, not grammar. Double-quoted passages and em-dash-led paragraphs are heuristic context hints, not reliable attribution. Apostrophes in names and possessives do not become quotation boundaries. Notes, research and footnote bodies have independent opt-in scopes.

Live work waits 0.9 seconds after a changed source is observed, or 2 seconds in low-power mode. One worker has at most two current/queued visible documents, with a bounded result channel. Superseded jobs are discarded; even before the next tick, stale source highlights are hidden and stale edits refused. There is no analyzer in draw/input, and hidden secondary sources do not receive work or edits.

Harper checks at most **65,536 source bytes per document** and displays up to **256 findings**. A larger source is explicitly **not checked**, not silently truncated or declared clear. Empty or excluded material does not initialize a model. One-off worker references go idle after 30 seconds without work. Cancellation discards work/results, but does not interrupt an already-running native parser; it may finish asynchronously. Shared dictionary and allocator memory may remain until exit. These bounds are software behavior, not physical battery or loaded-performance certification.

Settings: novel `.grimoire/writingdeck/checks.json`; personal `$HOME/.grimoire/writingdeck/checks-global.json`. Reads do not create either file. Unknown, corrupt, future, linked or externally changed metadata is preserved and reported. Metadata hashes are not authentication.

## Optional additional lesson

Original lesson 26 still records spelling, not grammar. Supplemental lesson 40 requires a genuine current grammar replacement in the original safe-demo manuscript; opening, consenting or navigating alone cannot complete it. Its launcher inserts no prose and grants no permission. Undo remains available, and skipping is allowed without penalty.
