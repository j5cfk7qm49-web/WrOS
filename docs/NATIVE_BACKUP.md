# Native encrypted backup and restore

Open **WrOS settings → Accounts → Backups** (also available under System →
Storage and from the dashboard backup widget). Choose **New backup on a local
drive** and enter a new absolute repository folder on your backup device, or
connect Google Drive and choose **New backup on Google Drive**. A folder on the
same filesystem does not protect against losing that disk.

Restic creates an encrypted repository and WrOS generates a private random
recovery key. **Export recovery key** writes a new file without replacing any
existing file. Keep that copy private and separate from the repository, ideally
on another safe device. Without the recovery key, the encrypted backup cannot
be restored. Keys and optional Google credentials are stored under
`$XDG_DATA_HOME/grimoire/wros/backup` (default `~/.local/share/grimoire/wros/backup`),
with private directory/file permissions; they never go in a manuscript.

**Create encrypted backup** first saves both manuscript buffers and refuses
unresolved source conflicts. A worker stages the entire project folder, including
notes, project-local metadata, history and empty folders. It checks that the saved
source stayed unchanged, creates an encrypted Restic snapshot, restores that
snapshot to a temporary folder, and compares all names, sizes and SHA-256 hashes
before recording **Restore checked**. These dates describe an actual prior restore
check, not a claim that a disconnected device or remote repository is presently
healthy. Local backups run without network access.

Select a snapshot to restore into a **new folder**. The original and all existing
destinations remain untouched. The destination is published only after Restic's
verification and the complete manifest comparison pass. A failed restore remains
under an explicit `.wros-restore-incomplete-*` name for review. Nothing opens the
restored folder automatically. To recover after losing a machine, install Restic
and WrOS, choose **Connect existing repository and recovery key**, select the
repository and your separately retained recovery file, refresh snapshots, and
restore into a new folder. This works without the original project or WrOS state.

Cancel stops the operation's process group. After a crash, use **Recover an
interrupted repository lock** to remove only Restic's stale locks, refresh
snapshots, and retry. A completed snapshot whose verification was interrupted
stays visible without a restore-checked receipt; an explicit successful restore
can qualify it. This release never prunes or deletes snapshots. Offline/missing
destinations report failure and record the saved project in the private offline
backup queue (`queue.json`) for explicit retry (`Run queued backups and retry
interrupted jobs`); you can also queue a saved project while offline (`Queue
saved project for offline backup`). Queued intent does not freeze the project: explicit retry backs up the latest saved
files and applies the open editor save/conflict gate. Uploads carry a durable queue
operation tag; interrupted uploads are restored and compared before avoiding a duplicate.
No unprompted background network upload
runs during ordinary writing. Interrupted temporary staging can need manual removal from
the private backup state directory after a hard kill or power loss.

The supported project limit is 5 GiB, 100,000 entries and 64 directory levels;
repositories with over 1,000 snapshots require external Restic maintenance.
Symlinks, special files and linked Git worktrees are refused rather than silently
omitting external content. Ordinary self-contained Git history is included.
This is project-content recovery: Unix ownership, permissions, extended attributes
and hard-link identity are not part of its manifest guarantee. Private credential
files are protected by per-user permissions; a separate encrypted credential vault
and complete native account lifecycle are still open under issue 28.

## Google Drive connection

On a trusted computer with rclone installed, run `rclone authorize drive`, approve
access in the browser, and paste **only the returned authorization JSON** into
WrOS's **Connect Google Drive** field. The native field is masked and Escape
discards it. Keep this response out of notes and messages, and clear the clipboard
after connecting. WrOS validates the connection with Google's quota endpoint
before replacing its dedicated private rclone configuration. Existing credentials
survive failure. Rclone owns refresh-token handling. Its default Drive
authorization requests Drive access; use a dedicated Google account if that
scope is unsuitable.

Use a destination such as `drive:WrOS-Backups`. Restic encrypts content and
snapshot metadata before rclone transmits repository objects. Active manuscripts
remain local; no Drive filesystem is mounted. Refreshing a Drive snapshot list,
creating a repository, backing up or restoring explicitly accesses Google.
Use **Verify Google Drive connection** to check provider reachability and detect
expired or revoked grants (`reauthorization-required` vs `offline`), **Reauthorize
Google Drive** to replace credentials atomically, or **Disconnect Google Drive
and remove local credentials** to attempt an overwrite and unlink `rclone.conf` locally
while keeping manuscripts and recovery keys intact. Revoke access through your
Google account's connected-app controls when needed; local credential removal
alone does not revoke a provider grant or guarantee secure deletion on SSDs or
copy-on-write filesystems. A real account,
provider interruption/refresh/revocation and cross-device Drive restore still
require live acceptance; local and synthetic-provider tests do not establish them.

## Maintenance CLI

`grimoire wros backup status` reads local configuration without provider access.
`list` explicitly refreshes snapshots; `create REPOSITORY` initializes a new
repository; `connect REPOSITORY KEY_FILE` connects an existing one;
`snapshot PROJECT` backs up an already saved project; `restore SNAPSHOT NEW_FOLDER`
restores a selected full snapshot ID; `export-key NEW_FILE` retains a recovery
copy, `queue PROJECT` records saved-project backup intent in the offline queue, `run-queue`
executes pending queued backups with interruption-safe deduplication,
`clear-queue` clears pending queue entries, `check-google` verifies Google Drive
credentials, `disconnect-google` scrubs and removes local Google credentials,
and `unlock` removes stale locks. Close the editor before CLI snapshots;
native snapshots enforce the editor save/conflict gate themselves. Tokens and
password values are never command arguments.

Maintained contracts: [Restic rclone backend](https://restic.readthedocs.io/en/v0.18.0/030_preparing_a_new_repo.html#other-services-via-rclone),
[Restic restore](https://restic.readthedocs.io/en/v0.18.0/050_restore.html),
[rclone Google Drive](https://rclone.org/drive/).
