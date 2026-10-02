lazysync 0.1.3 adds a compact tray companion for one GitHub repository at a time.

- PAT authentication through the native OS credential vault.
- Remembered checkouts, empty-folder cloning, and personal private repository creation.
- Manual sync by default; optional file-change and folder-idle automation.
- Durable recovery journals, stash preservation, normal pushes, and explicit conflict choices.
- Read-only pending diffs and paginated commit history.

Download the Windows x64 `.exe` installer (or `.msi`), or the `.dmg` for your Mac: `aarch64` for Apple Silicon and `x64` for Intel. Git must be installed and available on PATH.

Signing is optional in CI. Installers built without signing secrets are unsigned and may prompt operating system trust warnings. This release does not include automatic updates.
