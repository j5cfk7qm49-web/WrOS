# Technical, security and licensing risk register

All current verification is software/simulation/VM unless explicitly stated otherwise. No physical laptop or Google account was used. No real disk was formatted.

| Risk | Severity | Mitigation / acceptance evidence | Remaining gate |
|---|---|---|---|
| Manuscript overwrite from two panes or external sync | Critical | Exclusive open-view canonical/dev-inode registry, pre-save/adoption/replacement validation, existing guarded saves, both-buffer flush/recovery and 28 new UI cases plus actual PTY | In-process ownership is not an OS lock; check-to-rename races with unrelated writers and full kernel power-cut qualification remain open |
| Pin/source divergence or misleading anchor | High | Metadata-only references; explicit owner-aware edits; missing/ambiguous anchors and retained-editor aliases refused; guarded transaction rollback; 31 UI/14 core tests and actual-binary PTY/restart | No OS lock/hostile filesystem-race defense, per-workspace/global sets, full CommonMark section clipping or large-note/flash-write performance qualification |
| Misleading or lost journal observations | High | Sampled clean saved-Project counts, optional unknown baselines/daily net, immutable core append API, explicit pending retry/force-exit warnings, separate guarded reflection source; 23 UI/10 core + compiled-app PTYs | Not every edit, gross/time ledger, atomic filesystem-wide snapshot, OS file lock or power-loss/large-log qualification; sampled/pending facts can be lost on crash/force |
| Disk loss from installer/demo | Critical | Image scripts only write checked regular artifacts in `build/appliance`; installer plan has no formatter; staged installer refuses `/` | Production upstream-installer integration, fixture tests and physical confirmation flow |
| Unauthenticated lost laptop or suspend | Critical | Development wrapper refuses root; production design uses PAM login + LUKS + upstream console lock | **VM autologin is insecure by design. Do not deploy.** Screen lock/PAM/suspend integration and hardware tests outstanding |
| Root command injection | Critical | Exact pkexec executable; only layout IDs/tokens; absolute commands/argument arrays; no shell, arbitrary paths or sudo grant; isolated Python helper | Independent security review and malicious-client policy tests beyond current VM eligibility check |
| Keyboard lockout or client crash during preview | High | Root-owned committed layout; systemd timer armed before loadkeys; monotonic deadline; token-bound commit; crash-after-commit logic; rootless injected tests and actual VM revert | All modifiers, custom layouts, ANSI/ISO hardware, resume and emergency recovery on actual laptops |
| Broken boot/firmware chain | High | Whole UEFI disk boot tested with OVMF; maintained Debian kernel; recovery menu design | Signed shim/GRUB chain and physical Secure Boot; legacy BIOS not tested |
| Failed OS update or offline upgrade | High | Use apt, upstream signing, retained kernel/recovery design; never invent a package manager | Update UI, Btrfs/LVM snapshot or A/B decision, interrupted upgrade and restore fixtures; no automatic rollback claim |
| Heavy analysis slows typing/battery | High | Off-thread bounded work, optional one-shot Python, 30s CPU/60s wall guards; current native/proselint/Nabokov CPU/RSS evidence | Optimized/low-power physical input, battery and concurrent-model RAM qualification; trained startup reaches its CPU guard under TCG |
| Stale/wrong diagnostics damage prose | High | Tested exact Unicode/Markdown mappings, source/configuration/owner rejection, dialogue policy, equivalent-rule provenance and explicit native undo | Broader literary corpus and independent review; recommendations remain advisory and no literary-quality score is inferred |
| False productivity/continuity certainty | Medium | Actual recorded M5 ranges/civil dates and M7b source-bound metrics; unknown/self-reported facts stay distinct | Large-log archival, crash-tail/power-cut and physical qualification; M8 explicit continuity remains unfinished |
| Cloud privacy and lossy Google Docs | Critical | No cloud enrollment or manuscript transmission added; existing conflict-safe Markdown APIs preserved | Credential storage threat model, real OAuth/restore/conflict/round-trip tests, user consent |
| Footnotes exported as plain text/endnotes | High | Baseline audit explicitly does not certify footnotes | OOXML `footnotes.xml` relationships plus LibreOffice/PDF page placement tests |
| Missing licenses / ShareAlike spillover | High | Source manifest marks unbundled candidates; tutorial prose is original; preserve dictionary and Debian notices | Every actual corpus item needs specific edition/hash; derived CC BY-SA collections kept under compatible license separately from code; worldwide PD rights reviewed |
| Incomplete binary/OS redistribution notices | High | Machine-readable Cargo/Debian inventory and license notice generator; upstream copyright files retained | Corresponding source obligations for GPL components and release audit; manifest is not legal clearance |
| Reproducible recipe mistaken for identical build | Medium | Actual package lock/checksum recorded; docs distinguish workflow from bitwise reproducibility | Pin snapshots/toolchain/models, normalize filesystem metadata and sign release inputs |

## Practical implementation lessons

The first VM attempts exposed a GRUB embedded-prefix/config loading problem; embedding the final menu removed the dependency on a missing external module. Image assembly now resets regular artifact files to zero length before sparse reconstruction so stale bytes cannot survive a rebuild. Keymap generation also required `loadkeys --unicode --bkeymap` for container-safe validation. These are tested corrections, not reasons to skip actual boot verification.

The first real console screenshot exposed lingering login/splash characters where a terminal does not implement an alternate screen. The application now explicitly clears the terminal once during initialization. Keep VM screenshots as actual evidence, not interface mockups.

## When to ask for authorization

Ask before any physical partitioning/formatting, LUKS setup/recovery-secret handling, privileged live-host installation, authentication/credential exchange or intentional hardware power action performed by the development agent. Writing code and testing disposable regular-file VM artifacts is not authorization to install on the user's laptop.
