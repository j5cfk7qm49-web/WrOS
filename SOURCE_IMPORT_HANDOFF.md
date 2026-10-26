# WrOS — Writer’s OS: source-import handoff

**Product name:** WrOS — Writer’s OS. The former working title “WritingDeck OS” is historical. Grimoire is the underlying novel editor and retains upstream attribution.

**Current GitHub state:** This repository is the designated remote, but the recovered development source has **not yet been pushed** to GitHub. `main` contains coordination documentation; it is not a bootable OS or the development source.

## Recovered project archive

The Git-ready handoff archive is supplied separately in the ChatGPT project conversation under the name **`WrOS_Writers_OS_Codex_Handoff.zip`**. It contains the full original Git history and the interrupted development source, including:

- `writingdeck/foundation` at **`3360ec5`** — last historically qualified M7a branch. Do not change or re-label M7a results as fresh.
- `writingdeck/m7b-wip` at **`461bb64`** — the unverified M7b prose/WordNet/author-voice/revision work (`f253521`) followed by a **documentation-only WrOS branding handoff**. M7b still needs a full build and tests.
- `CODEX_PROMPT.md`, `docs/CODEX_CONTINUATION.md` — full Codex continuation prompt.
- `docs/WROS.md`, `docs/NAMING_MIGRATION.md` — official identity, roadmap entry, and compatibility notes.
- `tools/push-wros-source.sh` — non-force Git source-branch push script; the old script name remains as a compatibility wrapper.

The recovered archive was ZIP-integrity checked and extracted; the Git history passed `git fsck --full`, and the local working tree was clean. These checks do **not** qualify the unverified M7b implementation or the OS for installation.

## Import procedure

1. Provide the above ZIP to a Codex/Work environment with Git and authenticated access to this repository. Check for secrets, personal manuscripts, proprietary files and license compliance **before pushing to this public repository**.
2. Verify the archive's `.git` directory, branches, commit heads, and clean worktree (`git fsck --full`, `git status`, `git log --all`).
3. Run `bash tools/push-wros-source.sh` or equivalent Git transport. **Push both source branches without force** and do not overwrite the unrelated placeholder `main` history.
4. Verify source trees and exact branch heads in GitHub. Then safely choose the verified branch as default using supported repository controls, or reconcile the placeholder history without erasure.
5. Close [issue #1](https://github.com/j5cfk7qm49-web/WrOS/issues/1) **only after the source is remotely present**. Advance to [issue #2](https://github.com/j5cfk7qm49-web/WrOS/issues/2) to qualify M7b. See [issue #14](https://github.com/j5cfk7qm49-web/WrOS/issues/14) for the *tested* user-visible renaming across the app and OS.

The full [14-issue tracker](https://github.com/j5cfk7qm49-web/WrOS/issues) covers installation/authentication, locking, recovery/updates, long-form fiction workflows, verified footnotes, backups, tutorials, content licensing, CI and the naming migration.

**No real disk writes, credential enrollment, or manuscript uploads are authorized by this handoff.** The current project is not production-ready Linux installation media.
