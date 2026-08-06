# Changelog

## 0.18.0

- Added structured command and removal-preview states, including explicit
  blocked and successful-no-op transactions.
- Reconstructed retained APT, Pacman, Zypper, and DNF history across removals
  and reinstalls.
- Re-previewed cleanup-sensitive commands and aborted transaction drift before
  execution.
- Hardened XDG cleanup roots and filesystem deletion against path replacement.
- Added outcome verification, partial-result reporting, runtime protection,
  immutable-host recognition, and privileged executable pinning.
- Added desktop-name resolution, named Flatpak installations, Nix profiles,
  uv tools, Conda environments, container exports, and more local archives.
- Added JSON reports, batched disk sizing, diagnostics, CI, security guidance,
  checksum verification, and atomic installation.
