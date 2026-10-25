# Feature matrix

Status is per capability, not a blanket claim that a whole phase is complete. **Existing** = audited upstream; **Planned** = no implementation yet; **In progress** = partial; **Implemented** = code awaiting verification; **Tested** = named software/simulation/VM evidence; **Hardware verified** = identified physical machine; **Blocked** = specified external/technical gate. No feature is currently hardware verified.

Machine-readable inventory: [feature-status.json](feature-status.json). Details: [STATUS](STATUS.md), [AUDIT](AUDIT.md), [ROADMAP](ROADMAP.md), [TESTING](TESTING.md).

| Area | Capability | Status | Evidence / scope | Remaining limit |
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
| Home | Journal/pins/scratchpad/handoff/challenge/achievement/revision widgets | Planned | None | Depends on actual domain services |
| Workspaces | Five named dual-editor workspace slots + custom clones/width/cursors | Tested | Schema-v2 round-trip/v1 migration + dual restart/role-swap UI tests | Both selections/free scroll, active role/visibility/ratio; full preset content remains planned |
| Workspaces | F10 outline toggle and strict Ctrl-D focus | Tested | Hidden-pane geometry/focus and persistence tests | Strict Primary only; hidden Secondary/reference cannot receive input |
| Workspaces | Full editable secondary document, independent undo/autosave/selection | Tested | 28 new UI cases + core registry tests + real-binary PTY at four sizes | F11 enabled; existing guarded saves/dual recovery; in-process ownership, not an OS lock |
| Workspaces | Sticky sidebar, multi-pane ratios and full content presets | In progress | Two-editor ratio and pin sidebar/tiny/strict-focus tests | Ratios and responsive pin sidebar implemented; per-workspace pin sets and full content presets still planned |
| Notes | Single-source pins/scopes/cards/categories/trash/anchors/scroll | Tested | 14 core + 31 UI + 1 alias; actual binary PTY/restart at four sizes; read-only CLI | Metadata-only project references, explicit guarded source edits, scoped cards/sidebar/dashboard; unknown/failure preservation; no cross-process lock or large-note performance claim |
| Notes | Accurate auto/manual journal, multi-note scratchpad, handoff | Planned | None | Automatic event facts separated from reflections |
| Productivity | Basic daily/book/scene goals, weekly net pace/forecast | Existing | Upstream/current tests | Net growth, not gross/deleted/active-WPH |
| Productivity | Baseline Pomodoro and word/time sprint | Existing | Upstream/current timer/sprint tests | Advanced no-editing/revision variants planned |
| Productivity | Gross/net/deleted, idle-aware time, revision/footnote accounting | Planned | None | Requires edit ledger and injected monotonic clock |
| Productivity | Advanced goals/deadlines + offline 50k November calendar/rest days | Planned | None | No external account/endorsement |
| Productivity | Optional non-punitive achievements and advanced sprints | Planned | None | Fully disableable, no required streaks |
| Productivity | Persisted daily challenges across ten categories | Planned | None | Civil-date/DST/timezone/reroll tests required |
| Story Spark | Specific source/license research and exclusion manifest | Tested | Manifest/source review | No third-party challenge corpus bundled yet |
| Story Spark | 1,000+ substantive curated plot options + exercise corpus | Planned | None | No inflated record-count completeness claim |
| Story Spark | Four modes, compatible combinations, roulette/static, history/favorites/actions | Planned | None | Offline non-AI design; tutorial lessons disabled until real |
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
| Tutorial | 39-lesson catalog, safe demo, replay/skip/progress, help links | Tested | Core + actual editor/UI tests | 18 event-capable lessons including actual pin creation/anchored scroll return and Secondary edit; 21 explicitly unavailable |
| Tutorial | All 39 complete interactive exercises across final features | In progress | Continuous foundation only | Must land alongside each later feature, not wait until release |
| Qualification | Offline whole-VM operation and low-memory initial measurement | Tested | No network devices; guest process RSS in evidence | Does not include Python/spaCy or physical battery benchmark |
| Qualification | Physical laptop keyboard/Wi-Fi/lock/suspend/battery/update qualification | Blocked | No hardware available in this environment | Record model/firmware before claiming Hardware verified |
