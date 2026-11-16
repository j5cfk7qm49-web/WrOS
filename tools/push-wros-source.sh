#!/usr/bin/env bash
# Pushes source branches only. NEVER modifies or force-pushes main.
set -euo pipefail
cd "$(dirname "$0")/.."
REMOTE="https://github.com/j5cfk7qm49-web/WrOS.git"
if [ -n "$(git status --porcelain)" ]; then
  echo 'ERROR: working tree dirty. Review and commit/stash before pushing.' >&2
  exit 2
fi
case "$(git rev-parse writingdeck/foundation)" in
  3360ec5*) ;;
  *) echo 'ERROR: historical M7a verification pointer changed.' >&2; exit 2 ;;
esac
if git remote get-url wros >/dev/null 2>&1; then
  if [ "$(git remote get-url wros)" != "$REMOTE" ]; then
    echo 'ERROR: wros remote points elsewhere; inspect manually.' >&2
    exit 2
  fi
else
  git remote add wros "$REMOTE"
fi
printf '%s\n' 'Inspecting branch heads and remote; this will never force-push main.'
git log -1 --format='%h %s' writingdeck/foundation
git log -1 --format='%h %s' writingdeck/m7b-wip
git ls-remote wros HEAD 'refs/heads/*'
printf '%s\n' 'Pushing source branches without force...'
git push -u wros 'refs/heads/writingdeck/foundation:refs/heads/writingdeck/foundation'
git push -u wros 'refs/heads/writingdeck/m7b-wip:refs/heads/writingdeck/m7b-wip'
printf '%s\n' 'Verify https://github.com/j5cfk7qm49-web/WrOS/branches before changing the default branch.'
