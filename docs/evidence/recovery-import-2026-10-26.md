# Recovered source import — 2026-10-26

The recovered ZIP was extracted without replacing the existing workspace Git
directory. All archive entries passed ZIP CRC verification and path-containment
checks. `git fsck --full` passed (two unreachable recovery objects remain local).
The only extracted working-tree differences were executable-bit loss on two
shell scripts; tracked file contents matched the checkpoint. Windows-local
`core.filemode=false` avoids recording that extraction artifact.

Before publication, checksum-verified Gitleaks 8.30.1 scanned the working tree
(20.41 MB) and both branches' history (170 diff-bearing commits, 16.64 MB), with
zero findings. The branches contain 193 commits including merge/history nodes.
All 3,252 reachable object paths were also checked for credential-like names;
none matched. This is a bounded scanner result, not an independent security
certification. No build cache, local Git configuration, hooks, reflogs, dangling
objects, or upstream release tags were pushed.

The root MIT license retains Josh King's copyright. Dictionary, Rust, Python,
model and original-content notices remain separate; source publication does not
resolve the documented OS redistribution, independent editorial/legal, or
corresponding-source release gates.

An atomic normal Git push published only the two requested branches. An
independent no-tags bare clone from GitHub then passed `git fsck --full` and
verified:

| Branch | Exact head | Ancestor count |
|---|---|---|
| `writingdeck/foundation` | `3360ec511caa9004547a9c959ac09ea2cc90ff9d` | 191 |
| `writingdeck/m7b-wip` | `461bb64434e524fd3a8dba8a42568b3e86839d21` | 193 |
| `main` (unchanged placeholder) | `26b4e6aecf63f28a7c9e493c8888320fd1c4f57a` | 1 |

The M7b tree has 657 tracked files, including source, docs, original Spark packs,
license manifests, retained notices and `.github/workflows`. Foundation is the
default branch, as requested by import issue #1; its qualification remains
historical. M7b remains explicitly unverified. New work uses
`codex/m7b-qualification`, based on the preserved M7b head.

Neither an import nor an installed toolchain qualifies a new build, OS image,
physical installation, or release. Current runs and remaining work are recorded
separately from the archived M7a evidence.
