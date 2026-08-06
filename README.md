# uninstall

Find, explain, and safely remove Linux software—regardless of how it was installed.

```console
$ uninstall Dosbox
Checking installed applications…

Found 1 likely installed option:

   1. DOSBox Staging  [DNF]  0.82.2-5.fc44 | system | weak dependency | 16 MiB
      provides command: /usr/bin/dosbox
      Why installed: wine recommends it; DNF transaction 193: dnf install wine (recorded reason: Weak Dependency; source repository: updates)

Automatically selected the only result.
Checking removal impact…

Also expected to remove 8 now-unused dependencies: SDL2_net, fluid-soundfont-common, fluid-soundfont-gm, fluidsynth-libs, iir1, mt32emu, opusfile, speexdsp

Ready to run (freeing about 159 MiB):
  /usr/bin/sudo -- /usr/bin/dnf5 -y remove dosbox-staging.x86_64

Continue? [y/N]
```

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/JaredTweed/uninstall/main/install.sh | sh
```

The installer downloads the static binary for your architecture, verifies its
published SHA-256 checksum and version, and atomically installs it to
`/usr/local/bin`. It asks `sudo`, `doas`, or `pkexec` for access only when
needed.

For a user-only installation:

```sh
curl -fsSL https://raw.githubusercontent.com/JaredTweed/uninstall/main/install.sh |
  PREFIX="$HOME/.local" sh
```

Ensure `$HOME/.local/bin` is on your `PATH`. Release binaries are available for
x86-64, ARM64, ARMv7, and 32-bit x86, with matching checksum and Sigstore
verification files.

## What it does

- Finds applications, commands, package IDs, and matching local package archives.
- Resolves commands to their owning packages—for example, `aafire` to `aalib`.
- Explains why a package exists using current dependency data and retained install history.
- Previews the package manager's removal transaction and warns about dependents, unused dependencies, and protected packages.
- Shows the exact command and estimated disk space before asking for confirmation.
- Optionally removes selected package-manager data and detected user-data paths.

A single match is selected automatically, but removal is never automatic.
Pressing Enter at a confirmation prompt cancels.

## Supported installations

| Type | Backends |
|---|---|
| Universal apps | Flatpak, Snap, AppImage, Gear Lever |
| Debian and RPM | APT/dpkg, APT-RPM, DNF/DNF5/microdnf, YUM, RPM, rpm-ostree, Zypper, URPMI |
| Other native managers | Pacman, APK, OPKG, XBPS, Portage, Slackware, Eopkg, Swupd |
| Developer and user tools | Homebrew, Cargo, pipx, uv, global npm, Conda, Micromamba |
| Declarative profiles | Nix, legacy Nix, Guix |
| Local archives | `.rpm`, `.deb`, `.apk`, `.ipk`, `.opk`, `.xbps`, `.eopkg`, Flatpak, Arch and Slackware packages |
| Other | Standalone executables on `PATH` |

Distrobox and Toolbox exports are identified and blocked from being mistaken
for removable host executables. Named Flatpak installations, Gentoo Prefix,
third-party Swupd bundles, and read-only SUSE systems are also handled.

## Use

```sh
uninstall FreeCAD                 # search by app name
uninstall aafire                  # search by command
uninstall org.freecad.FreeCAD     # search by package ID
uninstall ~/Downloads/example.rpm # identify an installed local package
uninstall --show-dependencies lib # include fuzzy dependency matches
uninstall FreeCAD --json          # read-only machine output
uninstall FreeCAD --debug         # include detector diagnostics
uninstall --self-uninstall        # remove this command
uninstall --help
```

Search is Unicode-aware and case-insensitive. Very short queries require an
exact component match so a search such as `rg` does not return hundreds of
unrelated packages. Fuzzy dependency matches are hidden when ordinary app
matches exist; exact matches and command owners are never hidden.

Passing a local archive reads its metadata and checks whether the same package
and architecture are installed. The archive is not treated as proof of the
original installation source and is never deleted.

## Clear installation reasons

Every result gets one `Why installed:` line using the strongest locally
available evidence. Depending on the backend, this can include:

- An explicit, dependency, weak-dependency, or package-group reason.
- The application or dependency chain that currently requires it.
- The original APT, DNF/YUM, Pacman, Zypper, Flatpak, Snap, or Homebrew event.
- A DNF environment and subgroup, including the original transaction command.
- Repository, remote, channel, tap, profile, registry, Git, or update metadata.
- An explicit statement that retained history or an unmanaged file's source is unknown.

Examples:

```text
Why installed: wine recommends it; DNF transaction 193: dnf install wine (recorded reason: Weak Dependency; source repository: updates)

Why installed: installed through COSMIC Desktop Environment → COSMIC Desktop Supplementary Applications (cosmic-desktop-apps); DNF transaction 224: dnf install @cosmic-desktop-environment
```

Historical events and current package state are kept distinct. Missing history
is reported as unavailable rather than replaced with a guess.

## Removal safety

`uninstall` fails closed when it cannot establish a safe removal:

- Search and explanation never change package-manager state or refresh repositories.
- Native dry runs preview the transaction whenever the manager supports reliable simulation.
- Unknown and high-impact operations require a typed confirmation phrase.
- Core paths such as `/usr/bin` are never offered for direct file deletion.
- Unmanaged files can be removed directly only from application locations such as the home directory, `/usr/local`, and `/opt`.
- Package-manager executables are resolved and pinned before crossing a privilege boundary.
- The transaction is fingerprinted, repeated immediately before execution, and aborted if it changed.
- Selected application data is deleted only after every removal command succeeds.
- Installed state is checked again after execution.

On rpm-ostree systems, only layered packages are ordinary removal targets; base
image packages are not silently converted into `override remove` operations.
Portage uses dependency-aware `--depclean`, and XBPS never forces reverse
dependencies. Managers without a reliable machine-readable dry run remain
explicitly unknown-impact.

Do not run the whole program with `sudo`. It invokes a supported privilege
helper itself only for the selected system-wide operation.

## Associated data

Package-manager cleanup and detected user paths appear together:

```text
Remove associated data too? (optional)

   1. [Flatpak] Sandbox data and permissions       2.6 MiB
   2. [Detected] /home/jared/.cache/FreeCAD        0 B
   3. [Detected] /home/jared/.config/FreeCAD       28 KiB

Flatpak data is manager-owned. Detected paths are name matches, are not
guaranteed to belong to this app, and will be deleted permanently.
Choose numbers separated by commas, 'a' for all, or Enter to keep everything.
```

Nothing is selected by default. Detected paths are exact name or executable
matches under the XDG config, cache, data, and state locations; they are not
claimed ownership. Each path and cleanup action is shown again before removal.

Manager-owned cleanup follows that manager's semantics:

- APT `purge` removes package-managed system configuration, not home data.
- Flatpak `--delete-data` removes sandbox data and permission-store entries.
- Snap `--purge` skips creation of a recovery snapshot.
- Eopkg `--purge` removes changed package-managed configuration.
- Homebrew Cask `--zap` removes declared associated files and may include shared files.

Disk-space figures are best estimates. Shared objects, hard links, package
caches, and unreadable paths can change the amount ultimately reclaimed;
unknown sizes are never silently counted as zero.

## Automation

`--json` is always read-only. Guarded non-interactive removal requires one
exact package ID, its backend, and an exact authorization phrase. It refuses
blocked, unknown, and high-impact transactions.

```sh
uninstall ed --backend APT --confirm 'REMOVE APT:ed'
```

## Limitations

- Shell aliases, functions, and built-ins are not installed executables and cannot be discovered from another process.
- Dependency explanations are best effort: conditional dependencies, alternatives, removed history, and declarative configurations may not retain the original human intent.
- Standalone executables do not reliably reveal related files, so only the identified executable is offered automatically.
- Slackware does not track dependencies; Eopkg and Swupd cannot provide a fully reliable read-only removal transaction.
- On transactional SUSE systems, removal takes effect in a new snapshot after reboot.

When evidence is incomplete, the program says so and requires stronger
confirmation instead of claiming the operation is safe.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

Unit and CLI tests use disposable files. CI also performs real removals inside
isolated Debian, Ubuntu, Fedora, Alpine, Arch, openSUSE, and Void containers
using the same static release binary.

## License

MIT
