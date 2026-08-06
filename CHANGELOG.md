# Changelog

## 0.19.0

- Reimplemented the complete command as a memory-safe Rust application with a
  self-contained static binary and no runtime interpreter dependency.
- Preserved cross-manager discovery, command ownership, concise installation
  provenance, dependency-aware previews, protected-package escalation,
  combined associated-data selection, space estimates, transaction
  fingerprints, guarded automation, and self-removal.
- Added descriptor-backed cleanup snapshots, atomic directory isolation,
  executable pinning at privilege boundaries, bounded concurrent discovery,
  strict command timeouts, Unicode matching, and terminal-output sanitization.
- Added native release binaries for x86-64, AArch64, ARMv7, and i686 plus
  checksum and keyless Sigstore verification assets.
- Added Rust unit/CLI tests and real removal smoke tests across major Linux
  distribution families.

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
