# rc23: thin updates and no dashboard (design)

Owner decisions 2026-11-21 (chat). The owner approved the approach summarised in chat: #169 as written but with no second host, and the dashboard retired with its parts spread out. Calls marked "(my call)" are new. Laptop: HP Stream 11 on rc21; rc22 published 2026-11-21.

## Why

- **Thin updates (#169):**
  - Every update today is one signed bundle carrying every package that changed since the laptop's version. Most of that is upstream software (mpv, yt-dlp, deno…) that cache.nixos.org already serves byte for byte.
  - We host all of it on GitHub Pages: 100 MB per file, 1 GB per site.
  - The owner wants updates that don't depend on hosting everything ourselves.
- **The dashboard:**
  - The owner doesn't use it. They want one interface, the writing desk, rather than big monolithic screens.
  - The calendar and the rest of the productivity tools still live on one full-screen tabbed page in the dashboard's style.

## Goals

1. **Thin online bundles.**
   - The bundle WrOS hosts holds only the signed `InRelease`, `closure-manifest.json` and the files of store paths cache.nixos.org doesn't have: Grimoire, WrOS helpers, patched greetd, Cage and triggerhappy, the system configuration and anything else WrOS builds.
   - Every other path's `.narinfo` and `.nar.*` are listed in the signed manifest with their SHA-256, size and the address they come from on `https://cache.nixos.org/`. The engine downloads them from there.
   - Every file, hosted or upstream, is checked against the SHA-256 in the signed manifest before it is imported, exactly as today. The trust model doesn't change: our signature covers every byte, and cache.nixos.org is just another untrusted host.
2. **USB sticks still work offline.** The release also publishes a **full** bundle, the current kind, as the release asset that the WROS-UPDATE stick uses. Only the online channel on Pages is thin.
3. **Unchanged:** the signature, Valid-Until (#151), the baseline check ("needs WrOS X first"), probation, rollback, staging and the download progress line (#167). Progress counts bytes from both hosts.
4. **No dashboard:**
   - The WrOS Home page and its widget list go, along with "Configure widgets", the widget layout page, F12 and its shortcut, **WrOS › Overview**, the palette's dashboard entry and the "open the dashboard at login" setting.
   - Esc on a WrOS page (Settings, Shortcuts, the tutorial, and so on) goes back to the writing, or to the page that opened it, never to a dashboard.
5. **Small productivity dialogs:**
   - The full-screen tabbed productivity page splits into separate dialogs, each opened on its own from **Tools › Progress**: Goals…, Calendar…, Sprints…, Edits…, Collection… and Challenges….
   - Each is a centred box the size of the other dialogs, with no "1 edits 2 goals…" tab row.
   - The palette lists each one.
6. **A calendar month grid:**
   - **Calendar…** shows one month as a grid. Arrows move between days, Enter marks or clears a rest day, `[` and `]` change the month, and days outside the writing schedule are dimmed.
   - The schedule's other settings stay in the same dialog, below the grid.
7. **The bottom bar shows today's state:**
   - When today is a rest day, the bottom bar says "rest day" where it shows today's goal.
   - The first time the desk opens each day, today's challenge (if one is set) is shown once as a passing message.
8. **The live stick:**
   - It opens on the desk with the demo project and a small dialog: **Install WrOS to disk** / **Try it first**.
   - **WrOS › Install WrOS to disk…** stays in the menu for later. (my call: the dialog shows once per boot.)

9. **No copy of the WrOS repository in the system.** (Added 2026-11-21 after measuring rc22. Proceeding unless the owner objects.)
   - rc22's update was 75 MB with no upstream packages in it: initrd 35.6 MB, a copy of the whole repository 17.9 MB, Grimoire 16.5 MB.
   - The repository copy (44 MB on disk) comes from `builtins.readFile "${wrosRoot}/…"` and `"${wrosRoot}/…"` in `guards.nix`, `polkit.nix`, `live.nix` and `boot.nix`. String interpolation of the flake root copies and references the whole tree.
   - Every such use becomes `wrosRoot + "/…"`, which copies only that file.
   - A check fails the build if the system closure contains the flake source.
10. **A boot image that stays the same between releases.**
    - The initrd is one 35 MB file, re-sent whenever any byte inside changes. Between rc21 and rc22 only about 19 KiB of it changed: WrOS's version-stamped boot parts.
    - The WrOS programs in the initrd are built without the version stamp, from only the sources they use, so the initrd changes only when boot code changes.
    - A test builds the system at two version strings and checks that the initrd store path is identical.

With 9 and 10, an update like rc22's drops from about 72 MB to about 20 MB (mostly Grimoire). Thin bundles (1) cut the releases that change upstream packages.

## Non-goals

- Mirrors or a second host. GitHub Pages stays the only channel host, because thin bundles shrink what it serves to a few MB a release.
- Updating from nixpkgs without a WrOS release.
- New productivity features beyond the split and the month grid.
- Removing pins, the journal, the scratchpad, handoff, Story Spark or the revision dashboard. They keep their own menu and palette entries.

## Anti-goals

- An update that installs a byte the signed manifest didn't name.
- A laptop that can't update offline from a stick.
- A writer who loses a feature because its only door was the dashboard.
- rc22's saved dashboard layout being rewritten or deleted. It stays in `workspaces.json`, unused, so going back to rc22 still finds it.

## Approaches considered

- **Thin updates:**
  - (1) Let Nix substitute upstream paths itself from cache.nixos.org, trusting its signing key. That needs `nix copy --from https://…`, a second trust root, and a check of each NAR hash afterwards.
  - (2) **Chosen:** list upstream files in the signed manifest with their cache.nixos.org address and SHA-256, and download and verify them like hosted files. One trust root (ours); the engine's verify and import code stays as it is.
  - (3) Host everything and ask for a bigger host: rejected by the owner.
- **Mirrors:** dropped, owner 2026-11-21. GitHub release assets can't be a fallback anyway, because the updater refuses redirects.
- **The dashboard:**
  - (A) **Chosen:** spread its parts into small dialogs, the bottom bar and the live-stick prompt.
  - (B) The same list in a side pane: still one monolithic list.
  - (C) The bottom bar only: the calendar and goals would have no place.
  - A "Today" view in the clearing carousel was dropped because it repeated the bottom bar (owner).

## Decided details

- **Manifest:**
  - An upstream entry is a normal `cache_files` entry whose `url` is `https://cache.nixos.org/<hash>.narinfo` or `https://cache.nixos.org/nar/<file>`.
  - `path` is still where it is staged (`cache/<hash>.narinfo`, `cache/nar/<file>`), and the staged `.narinfo`'s own `URL:` line already points at `nar/<file>`.
  - The engine's URL check accepts exactly the GitHub release-asset pattern (as today) and `https://cache.nixos.org/` followed by `[0-9a-z]{32}.narinfo` or `nar/[0-9a-z]{52}.nar(.xz|.zst|.bz2)?`.
  - No redirects, the size limits as today, and HTTPS only.
- **Builder** (`tools/build-nixos-update-bundle.py --thin`). For each kept path:
  - fetch `https://cache.nixos.org/<hash>.narinfo`;
  - if it exists and its `NarHash` equals our path's NAR hash, download the archive it names, check it decompresses to that NAR hash, and record both files' SHA-256, size and address;
  - remove the files from the bundle.
  - Paths the cache lacks, or whose NAR hash differs (anything WrOS patched or built), stay in the bundle.
  - The full bundle is built as today.
- **Tests:** the engine's upstream base URL is a constant with a test override (as `ONLINE_URL` is), so a VM test can serve a mini cache.nixos.org over plain HTTP. The production check refuses anything but `https://cache.nixos.org/`.
- **Release workflow:** publishes the thin tree to Pages and attaches the full tar to the release, as `wros-nixos-updates.tar`. The thin tar is a second asset, `wros-nixos-updates-thin.tar`, which the workflow deploys.
- **Help and docs:** `assets/help/27-the-menu.md`, `24-every-key.md` (F12 goes), the tutorial step "Open Overview with F12" is rewritten, the Settings help, and `docs/API_AND_NETWORK_USE.md` (cache.nixos.org is a new update host).

## Left to the builder

- How the builder caches narinfo lookups.
- The calendar grid's exact look at 80×24.
- Which WrOS pages return to which on Esc.
- Whether the productivity code keeps one `Overlay::Productivity { page }`, drawn small and without tabs, or becomes separate overlays.
