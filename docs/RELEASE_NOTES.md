lazysync 0.2.6 adds resizable windows with saved sizes and clearer Windows folder paths.

- The visible resize grip uses a pointer-drag fallback on macOS, where the native grip API is unsupported.
- Resize from window edges or the monochrome bottom-right grip, including in setup and settings dialogs.
- Focus the grip and use arrow keys to resize in 16-pixel steps, or Shift + arrows for 64-pixel steps.
- Compact and Difference checker views remember separate sizes. Sizes save after resizing and when hiding or quitting; the checker stays anchored to the right edge when opened or closed.
- Layouts fill the resized window, and usable monitor bounds account for taskbars and docks. Small screens and display scaling are handled without replacing remembered dimensions with automatically clamped sizes.
- Windows folder labels and inputs show familiar paths without the extended-path prefix. Native operations retain long-path support, and equivalent folder paths cannot create duplicate mappings or prevent saved setup from resuming.
- Window preferences live in a separate versioned, atomically saved window-state.json; repository settings, credentials, and recovery records are unaffected.

The previous minimalist interface, syntax-highlighted Local/Cloud comparison, file size information, ignore editor, and sync safety fixes are included.

Download the Windows x64 .exe installer (or .msi), or the .dmg for your Mac: aarch64 for Apple Silicon and x64 for Intel. Git must be installed and available on PATH.

Installers built without signing secrets are unsigned. This release does not include automatic updates.
