lazysync 0.2.4 catches nested Git repositories before they can be accidentally staged as submodules during an initial sync.

- Included nested repositories are detected and named before backup, stash, or staging begins.
- Submodule errors name the affected paths and explain how to sync them separately or exclude staged links while keeping their files and histories.
- A retry clears the previous error while validation and sync progress are displayed.

File size information and the fixes for repository operations getting stuck behind background checks are included.

- File sizes are visible in the ignore picker and Difference checker, including separate Local and Cloud sizes.
- Sizes reflect full files, even for binary files and truncated previews; hover to see the exact byte count.
- Files larger than 100 MiB have a clear label, and oversized-file errors include their sizes.

- Background checks no longer hash every file's contents; automation Off skips even the metadata fingerprint.
- Manual actions queue behind routine checks. Status and settings remain readable while the worker is running.
- Filesystem event storms are coalesced into one pending rescan and debounced.
- Interrupted operations release the busy state while preserving recovery records and backups.
- Sync displays readable operation stages and upload/receive progress, with limits on stalled network connections.
- Before sync changes anything, included files over GitHub's 100 MiB limit are named with directions to the ignore settings.

The minimalist interface, left-expanding syntax-highlighted Difference checker, ignore editor, and monochrome icons are included.

Download the Windows x64 `.exe` installer (or `.msi`), or the `.dmg` for your Mac: `aarch64` for Apple Silicon and `x64` for Intel. Git must be installed and available on PATH.

Installers built without signing secrets are unsigned and may prompt operating system trust warnings. This release does not include automatic updates.
