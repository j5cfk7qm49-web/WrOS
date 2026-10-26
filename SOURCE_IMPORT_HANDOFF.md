# WrOS source-import handoff

**Status:** This GitHub repository is the designated remote for WritingDeck OS, but the recovered development snapshot has **not yet been pushed** here. The initial `main` branch contains the placeholder README and this handoff.

## Recovered source

The recoverable Arena workspace has a complete local Git history. The last qualified implementation checkpoint is commit `3360ec5` on `writingdeck/foundation` (M7a: opt-in Harper grammar). The interrupted M7b prose/WordNet/voice/revision integration exists as **unverified working-tree modifications and untracked source**, not as a qualified release.

The recovered ZIP archive is available as a conversation attachment from the ChatGPT evaluation session. To continue in Codex/Work, attach the restored archive and use [Issue #1](https://github.com/j5cfk7qm49-web/WrOS/issues/1) to import the actual files and historical commits. This repository alone does **not** yet contain that source.

## Safe import

1. On a developer computer or in a Work/Codex environment with the recovered workspace and GitHub authentication, validate `git fsck --full`, `git status`, `.gitignore`, and secrets before publishing.
2. Preserve `writingdeck/foundation` at `3360ec5`, and separately checkpoint the interrupted M7b working tree as explicitly `WIP`/unverified.
3. Add `https://github.com/j5cfk7qm49-web/WrOS.git` as the remote and push the two branches **without force**. Do not overwrite `main` or erase the preexisting history.
4. Make the verified source branch the default using GitHub repository settings, or safely integrate the placeholder commit without discarding either history.
5. Verify the upload from GitHub: files, branches, LICENSE, commit ancestry, tests, and manifests.
6. Close Issue #1 only after the import succeeds; proceed to [Issue #2](https://github.com/j5cfk7qm49-web/WrOS/issues/2) to qualify M7b.

## Development tracking

See the [13 open issues](https://github.com/j5cfk7qm49-web/WrOS/issues) for the implementation backlog. All original-user manuscript data and any credentials must remain outside the public repository. The project is an incomplete development prototype, **not production-ready Linux installation media**.

Do not equate historical M7a test reports, archived ZIP integrity, or QEMU boot records with fresh M7b verification or physical-hardware certification.
