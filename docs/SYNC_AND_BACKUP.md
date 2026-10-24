# Saving, history, backup, synchronization and mobile

## Five distinct guarantees

1. **Local saving:** existing atomic guarded manuscript saves; works offline.
2. **Local history/recovery:** existing dated copies, Git writing sessions and crash/failed-save rescue. Valuable, but vulnerable to loss of the same device.
3. **Off-device backup:** versioned, verified copies with a tested restore. Existing optional Git remote push is not a new Google Drive backup service.
4. **Bidirectional sync:** compare versions, detect conflicts and preserve both sides. A simple upload/copy command is not this protocol.
5. **Google Docs interoperability:** a separate import/export/round-trip question, not implied by storing Markdown files in Drive.

Current cloud-related code detects known synced folders and preserves external edits/conflict copies. `grimoire-app` has stamp-checked mobile save APIs and the Android Tauri prototype uses them. There is no completed Google OAuth/Drive queue, iPhone client or safe Docs round trip in WritingDeck.

## Planned Drive backup (M9)

Prefer maintained rclone for explicit terminal OAuth enrollment rather than hardcoded client secrets. Verify the provider's supported current authorization flow; a browser on a trusted second device may be needed. Never ask users to paste access tokens into a manuscript or publish credentials. Keep rclone/account configuration private (0600), outside a synced project, with an optional secret store and revocation instructions.

Opt-in backup jobs stage a consistent saved revision, create versioned content, record a cryptographic SHA-256 manifest, upload only after explicit enrollment, verify the remote result and support a restore to a **new directory**. An interrupted job is resumable/idempotent and never relabeled verified until the hash/restore checks pass. Encryption before upload and recovery-secret custody require an explicit threat-model decision. Low-power/offline states keep the queue without blocking input.

Use rclone `copy`/versioned immutable destinations for initial backup evaluation, not a blind `sync --delete` of the only good copy. Bidirectional synchronization needs a base-version ledger, tombstone/conflict policy and safe application through existing guarded saves; never overwrite a dirty editor because a cloud watcher noticed a file.

## Mobile and Google Docs evaluation

Plain Markdown in Drive can be backed up and opened with a compatible editor; edit one device at a time initially, finish saves, and verify the conflict workflow. Rich Google Docs conversion may alter YAML frontmatter, Markdown syntax, footnotes, scene structure or comments. Prefer exported DOCX review copies plus explicit import into an experimental/new scene until real tests show otherwise.

Required round-trip fixture: Unicode, YAML unknown fields/comments, scene metadata, inline Markdown, contextual footnotes, private annotations and chapter order. Compare body/metadata/reference identity, not just rendered appearance. If conversion loses information, document and enforce a safer one-way review or manual merge workflow; do not promise faithful Docs synchronization.

## Status and credential gates

No Google account was enrolled, no manuscript transmitted and no live restore/Docs round trip performed. Live authorization and provider behavior are **Blocked** on explicit credentials/consent when that milestone is reached; the local fake transport/queue/conflict infrastructure can be implemented without credentials. Those new adapters are currently **Planned**, not yet code.

Current tests: upstream mobile/sync conflict, atomic save, deleted/unavailable file behavior and recovery tests. Future tests: fake transport interruption, hashes, replay-safe queue, restore-new-directory, conflict/tombstone handling and network-disabled writing. Tutorial lessons 36/37 remain unavailable until actual safe enrollment/restore/mobile workflows exist.

## M3b local two-buffer safety

Both open documents now share the existing guarded Project path. Tests cover a concurrently changed inactive document (external text adopted, local text parked), hidden-buffer autosave, failed-save replacement refusal, both crash-recovery drafts, canonical/hard-link/symlink ownership and external identity changes that pause saving/adoption without dropping either buffer. Clean external renames follow an unambiguous fingerprint; dirty missing files are parked before detachment. This is local safety, not an OS file lock, cross-process CAS, remote backup or successful restore from another device. See [WORKSPACES](WORKSPACES.md) for precise boundaries.
