# Feature status matrix

As of 2026-10-25 — 0.1.0-dev foundation + M3b + M4 + initial M5 + M6 software. **No hardware-verified claims.**

Existing means audited baseline; Tested identifies automated/VM evidence, not production qualification. See [STATUS](STATUS.md) and [ROADMAP](ROADMAP.md).

| Area | Capability | Status | Verification | Detail |
|---|---|---|---|---|
| Baseline | Markdown novels, YAML frontmatter, parts/chapters/scenes | Existing | Upstream/current Rust tests | Preserved; no manuscript-format rewrite |
| Baseline | Atomic autosave, dated history, recovery | Existing | Upstream/current failure and recovery tests | Local safety is not off-device backup |
| Baseline | External-file conflicts and guarded mobile saves | Existing | Core sync and grimoire-app integration tests | Existing Android prototype; no iPhone client claim |
| Baseline | Corkboard, find/replace, notes/TK, notebook names | Existing | Upstream/current tests | Keep existing conventions and shortcuts |
| Baseline | Locale spelling and personal/notebook vocabulary | Existing | Upstream/current spelling tests | Harper grammar not yet integrated |
| OS | Detailed audit, architecture, base decision and docs | Tested | Document/manifest checks | Debian 13 Stable chosen; no custom kernel |
| OS | Staged rootfs + regular-file GPT/UEFI image workflow | Tested | Actual QEMU/OVMF whole-disk boot | Reproducible recipe, not bit-identical signed release |
| OS | Boot-to-writing splash + non-root tty1 startup | Tested | Actual VM screen and UID assertion | Explicit VM demo autologin is not production authentication |
| OS | Conventional Linux development mode | Tested | Rust builds/tests + CLI smoke | No appliance policies installed in host |
| OS | Read-only compatibility/disk/account/timezone/layout installation plan | Tested | Rootless fixture tests | No apply mode; mounted/read-only/small/unknown disks refused |
| OS | Production disk/LUKS/account/Wi-Fi first-run installer | Planned | None | Delegate storage/authentication to upstream Debian installer; no physical install performed |
| OS | Authenticated boot + console screen lock / lock before suspend | Planned | None | Critical release gate; VM autologin must not ship |
| OS | Authenticated tty2 maintenance and usable recovery boot | In progress | Upstream unit structure + docs only | Normal Debian account required; demo passwords locked, so usable recovery not certified |
| OS | Signed OS/app updates, snapshots/rollback and offline bundles | Planned | None | Use apt plus a selected upstream snapshot mechanism |
| OS | Secure Boot / legacy BIOS / physical firmware compatibility | Blocked | UEFI without Secure Boot only | Requires signed boot-chain work and identified physical hardware |
| System | Typed Linux + explicitly simulated backend | Tested | Rust adapter/validation tests + worker UI test | No arbitrary shell or manuscript paths in OS requests |
| System | Cached/off-thread network/battery/brightness/storage/timezone status | Tested | Missing-device/fixture tests + networkless VM | Physical battery/backlight behavior not verified |
| System | Wi-Fi radio and scan command interfaces | Tested | Typed command/unit tests | Actual radio/enrollment unverified; scan truncation still open; nmtui fallback |
| System | In-app new-network credentials / secret agent | Planned | None | Never passwords in argv/logs |
| System | Brightness request + UI/OS power-saving distinction | Tested | Validation and no-device tests | No physical backlight or battery-savings claim |
| System | Confirmed power requests after successful manuscript save | Tested | TUI confirmation + command construction tests | Physical suspend/restart/shutdown deliberately not run |
| System | Clock/timezone changes, sound/volume, updater/restore panels | Planned | None | Read-only status is not full settings coverage |
| Keyboard | QWERTY/Colemak/DH ANSI/ISO console maps | Tested | Four Unicode map builds; VM QWERTY/DH/Colemak key assertions | Full modifiers/ISO physical keyboard remain unverified |
| Keyboard | 15-second preview, token commit and independent rollback | Tested | 13 rootless helper tests + real VM systemd timer | Survives client absence; commit generation handles crash boundary |
| Keyboard | Layout persistence / pre-login reapplication | Tested | Guest config commit and boot-command reapplication | Separate cold reboot/suspend hardware verification outstanding |
| Keyboard | Installer/first-run layout wizard and custom map registration | Planned | Planner records registered choices only | Arbitrary runtime keymap paths rejected |
| Keyboard | Safe custom shortcut aliases with legacy compatibility | Tested | TUI capture/persistence + logical/Unicode tests | Nine safe alias actions including Secondary; not all legacy binding removal/reassignment |
| Keyboard | Raw physical-position shortcuts through generic tty | Blocked | No reliable scan-code channel | Use logical characters and function/arrow keys; no double mapping |
| Home | Real existing-data dashboard widgets + hide/reorder/navigation | Tested | 80x24, 100x30, 120x40, 160x50 TUI tests | No fabricated metrics or unimplemented challenge widgets |
| Home | Optional startup home / direct primary restore | Tested | State tests + VM startup + recovery precedence | Newer upstream resume takes priority at startup |
| Home | Journal/pins/scratchpad/handoff/challenge/achievement/revision widgets | In progress | Original-source UI/PTY + six M5 tool routes and cached date summaries | Pins/journal/scratchpad/handoff plus recorded edits/goals/calendar/sprints/collection/challenges implemented; advanced revision diagnostics remain M7 |
| Workspaces | Five named dual-editor workspace slots + custom clones/width/cursors | Tested | Schema-v3 round-trip/v1-v2 migration, dual restart/role-swap and guarded starter/pin-set UI tests | Independent selections/free scroll and role/ratio; explicit empty-original starters and per-workspace card IDs, no implicit prose template |
| Workspaces | F10 outline toggle and strict Ctrl-D focus | Tested | Hidden-pane geometry/focus and persistence tests | Strict Primary only; hidden Secondary/reference cannot receive input |
| Workspaces | Full editable secondary document, independent undo/autosave/selection | Tested | 28 new UI cases + core registry tests + real-binary PTY at four sizes | F11 enabled; existing guarded saves/dual recovery; in-process ownership, not an OS lock |
| Workspaces | Sticky sidebar, multi-pane ratios and explicit workspace content/pin sets | Tested | Core/UI + four-size compiled-app starter/pin-set/external-layout preservation PTY | Five explicit starters, existing-source preservation, guarded layout rollback, reference-only pin sets; no cross-file atomicity/large-load claim |
| Notes | Single-source pins/scopes/cards/categories/trash/anchors/scroll | Tested | 14 core + 31 UI + 1 alias; actual binary PTY/restart at four sizes; read-only CLI | Metadata-only project references, explicit guarded source edits, scoped cards/sidebar/dashboard; unknown/failure preservation; no cross-process lock or large-note performance claim |
| Notes | Sampled auto/manual journal, multi-note scratchpad and next-open handoff | Tested | Journal 34 Rust; M4c 45 Rust; actual four-size compiled-app promotion/handoff process restart + read-only CLI | Original reflections/notes/intention; source-preserving independent scene with Prepared/Ready retry; no generated prose or gross/active-time ledger |
| Productivity | Basic daily/book/scene goals, weekly net pace/forecast | Existing | Upstream/current tests | Net growth, not gross/deleted/active-WPH |
| Productivity | Baseline Pomodoro and word/time sprint | Existing | Upstream/current timer/sprint tests | Existing baseline retained; M5 variants are separate recorded-metric tools |
| Productivity | Gross/net/deleted, idle-aware time, revision/footnote accounting | Tested | Actual-range/source-class core tests + injected clocks; 30 controller cases; compiled four-size edit/paste/replay/restart/retry PTY | Bounded source units, not native/rendered counts; observed inter-edit/keyboard-pair time, NOT exclusive human typing; checkpoint crash tail explicitly unclaimed |
| Productivity | Advanced goals/deadlines + offline 50k November calendar/rest days | Tested | Goal/calendar/forecast core + real four-size UI/PTY and persisted restart | Nine explicit native/measured goal choices; civil windows, rest days, required pace; forecast uses recorded work dates, not missing zeroes |
| Productivity | Optional non-punitive achievements and advanced sprints | Tested | Sprint/break/collection core + real UI/PTY/restart/failure tests | Five variants, pause/resume/retained breaks; source-status and actual learning awards; badge failure is optional, runtime checkpoints are not |
| Productivity | Persisted daily challenges across ten categories | Tested | Deterministic core DST/travel/filter/reroll tests + genuine measured UI/PTY/restart | 30 original MIT exercises; frozen date/profile/source/acceptance/reroll history; explicit measured versus subjective evidence; NOT the M6 plot corpus |
| Story Spark | Specific source/license research and exclusion manifest | Tested | Pinned original source/artifact registry + six Python provenance contracts | Historical OCR/modern cleanup and all third-party candidates unbundled; original AI-assisted M6 bundle has explicit review/rights limits |
| Story Spark | 1,000+ substantive curated plot options + exercise corpus | Tested | Offline compiler/--check, core exact-ID/hash/provenance tests and six Python corpus contracts | 1047 accepted independently worded plots / 84 exercises; 1051 literal drafts, four substantive-overlap holds. AI-assisted authoring/agent editorial review, not independent human/legal or semantic certification |
| Story Spark | Four modes, compatible combinations, roulette/static, history/favorites/actions | Tested | 28 Spark core + 28 real UI + adoption tests; four compiled PTY methods/four-size loaded restart | Offline non-AI; exact rerolls, civil-date daily, bounded references, real Markdown/pin/scratch/scene/timed sprint and optional SELF-REPORTED assignment; no M8 structured-task fiction |
| Writing tools | Harper grammar + dialogue/novel/global policy | Planned | Crate API/license evaluated only | harper-core 2.13.0 not linked into application |
| Writing tools | Nabokov fiction-aware profiles + native lightweight analysis | Planned | Source/runtime/license evaluated only | Optional Python/model costs not benchmarked |
| Writing tools | Standalone proselint versioned structured-output integration | Planned | 0.14/0.16 CLI differences evaluated | No Vale dependency; adapter not implemented |
| Writing tools | Unified deduped stale-safe diagnostics + revision dashboard | Planned | None | No automatic rewrite/quality score |
| Writing tools | Offline WordNet definitions/senses/synonyms/antonyms UI | Planned | Host package evaluated only | Not in minimal VM or current UI |
| Writing tools | Author voice profiles with sufficient-material safeguards | Planned | None | Informational deviations only |
| Long-form | Structured explicit continuity/timeline/knowledge/thread tracker | Planned | None | Definite contradictions require explicit facts |
| Long-form | Text-anchored annotations and revision queue | Planned | None | Separate from published manuscript |
| Long-form | Named milestones/experimental variants/diff/selective restore | Planned | Existing dated history reused later | No competing version store rewrite |
| Long-form | Local sentence-level read-aloud/highlighting/controls | Existing | Existing silent-engine tests | Installed voice engine/model may be needed |
| Long-form | Multi-scene/chapter read-through and mark-for-revision | Planned | None | No cloud speech dependency |
| Publishing | Native DOCX/EPUB/Markdown + print formatting | Existing | Existing output/package tests | Preserved |
| Publishing | PDF through optional headless LibreOffice | Existing | Existing conversion path; external-engine tests conditional | LibreOffice not in minimal image; no page-footnote certification |
| Publishing | Contextual Markdown footnotes + genuine DOCX/PDF bottom-page output | Planned | None | OOXML relationships/pagination fixtures required |
| Publishing | Unified pre-export link/asset/footnote/privacy validation | Planned | None | Some existing note/TK validation is not this full preflight |
| Cloud | Opt-in Drive OAuth, private credentials, upload queue, verified backup/restore | Planned | None | Local state/fake-transport infrastructure can be built without credentials |
| Cloud | Conflict-aware bidirectional sync + safe mobile/Docs evaluation | Planned | Existing generic file conflicts only | Copy/sync/Docs interoperability are separate guarantees |
| Cloud | Live Google authorization/restore/Docs round-trip acceptance | Blocked | No account enrolled or manuscript transmitted | Requires explicit user credentials and consent at M9 |
| Tutorial | 39-lesson catalog, safe demo, replay/skip/progress, help links | Tested | Core + actual editor/UI tests | 27 event-capable / 12 unavailable; M6 22–24 require real saved plot, exercise favorite and reroll/favorite/new scene chain. Context events are not OS/process or creative-completion attestation |
| Tutorial | All 39 complete interactive exercises across final features | In progress | Continuous foundation only | Must land alongside each later feature, not wait until release |
| Qualification | Offline whole-VM operation and low-memory initial measurement | Tested | No network devices; guest process RSS in evidence | Does not include Python/spaCy or physical battery benchmark |
| Qualification | Physical laptop keyboard/Wi-Fi/lock/suspend/battery/update qualification | Blocked | No hardware available in this environment | Record model/firmware before claiming Hardware verified |
