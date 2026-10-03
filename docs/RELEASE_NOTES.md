lazysync 0.2.7 fixes Windows sync failures while another process uses a project folder.

- Save local changes with native Git, avoiding libgit2's `Os/GenericError` when a terminal or development server uses a nested folder as its working directory.
- Record the saved stash even if locked files prevent cleanup. Sync pauses with recovery instructions instead of continuing with a partially cleaned checkout.
- Recover interrupted successful stashes by their operation marker. Existing user stashes and recovery backups remain protected.
- Release builds support the repository's current visibility and publish only after all platform installers succeed.

The resizable windows, saved window sizes, Local/Cloud comparison, ignored-file settings, and previous sync safety fixes are included.

Download the Windows x64 .exe installer (or .msi), or the .dmg for your Mac: aarch64 for Apple Silicon and x64 for Intel. Git must be installed and available on PATH.

Installers built without signing secrets are unsigned. This release does not include automatic updates.
