# uninstall

`uninstall` answers a surprisingly awkward Linux question: “How did I install
this app or command, and how do I remove it?”

```console
$ uninstall DOSbox

Found 1 likely installed option:

   1. DOSBox Staging  [DNF]  0.82.2-5.fc44 | system | weak dependency | 16 MiB
      provides command: /usr/bin/dosbox
      Why installed: wine recommends it; DNF transaction 193: dnf install wine (recorded reason: Weak Dependency; source repository: updates)

Automatically selected the only result.

Also expected to remove 8 now-unused dependencies: SDL2_net, fluid-soundfont-common, fluid-soundfont-gm, fluidsynth-libs, iir1, mt32emu, opusfile, speexdsp

Ready to run (freeing about 159 MiB):
  /usr/bin/sudo -- /usr/bin/dnf5 -y remove dosbox-staging.x86_64

Continue? [y/N]
```

```console
$ uninstall term
Checking installed applications…
Explaining installation and checking removal impact…

Found 4 likely installed options:

   1. Terminal  [DNF]  50.1-2.fc44 | system | group | 2.1 MiB
      ptyxis.x86_64
      provides command: /usr/bin/ptyxis
      Why installed: installed through Fedora Workstation Environment → GNOME (gnome-desktop); DNF transaction 2: dnf5 --config /kiwi_dnf5.conf -y --disable-plugin=priorities,versionlock --releasever=42 --exclude=device-mapper-multipath --exclude=fcoe-utils --exclude=gfs2-utils --exclude=g… (abbreviated)
   2. XTerm  [DNF]  406-1.fc44 | system | 1.9 MiB
      provides command: /usr/bin/xterm
      Why installed: explicitly requested; DNF transaction 149: yum -y install libxkbcommon-x11 libnl3 libnsl iptables psmisc libatomic xterm (recorded reason: User; source repository: fedora)
   3. Micro Text Editor  [DNF]  2.0.15-1.fc44 | system | external | 13 MiB
      micro.x86_64
      provides command: /usr/bin/micro
      Why installed: installed outside DNF and later recorded in its package database
      archive evidence: AppStream component io.github.zyedidia.micro
   4. COSMIC Terminal  [DNF]  1.5.0-1.fc44 | system | dependency | 36 MiB
      cosmic-term.x86_64
      provides command: /usr/bin/cosmic-term
      Why installed: cosmic-session requires it; DNF transaction 224: dnf install @cosmic-desktop-environment (recorded reason: Dependency; source repository: updates)

Choose numbers separated by commas, 'a' for all, or Enter to cancel.
> 2
Checking removal impact…

Also expected to remove 2 now-unused dependencies: libXaw, xorg-x11-fonts-misc

Ready to run (freeing about 8.5 MiB):
  /usr/bin/sudo -- /usr/bin/dnf5 -y remove xterm.x86_64

Continue? [y/N] 
```

It searches:

- Flatpak (including named system installations), Snap, AppImage, and
  AppImages managed by Gear Lever
- APT/dpkg, APT-RPM, DNF/DNF5/microdnf/YUM/RPM, URPMI, layered rpm-ostree
  packages, Zypper/RPM, and Pacman
- Alpine/OpenWrt APK, legacy OpenWrt OPKG, Void XBPS, Gentoo Portage
  (including Gentoo Prefix), Slackware pkgtools, Solus Eopkg, and Clear Linux
  Swupd and third-party Swupd bundles
- local `.rpm`, `.deb`, `.apk`, `.ipk`/`.opk`, `.xbps`, Slackware,
  `.eopkg`, Flatpak, and Arch `.pkg.tar.*` archives that correspond to
  installed packages
- Homebrew, named modern Nix profiles, legacy Nix and Guix profiles, Cargo,
  pipx, uv tools, Conda/Micromamba environments, and global npm packages
- applications exported from Distrobox or Toolbox, which are explained and
  safely blocked from being mistaken for unmanaged host executables
- standalone executables on `PATH`

You choose the exact result, decide separately whether its data should go too,
see the command that will run, and confirm once more. If a command has a
different package name—such as `aafire` from `aalib`—it asks the system package
database which package owns the executable. Manager shims, symlinks, and
alternatives are resolved before falling back to standalone removal. Shell
aliases and functions are not executables, so they cannot be identified from a
separate process.

If no manager owns a command, `uninstall` can remove its exact executable from
`PATH`. It labels this as `Standalone` because files installed alongside that
executable cannot always be inferred safely. AppImage format signatures are
recognized even when the file has no `.AppImage` extension; a verified command
symlink and its underlying image are then removed together.

Direct standalone deletion is limited to user and administrator application
locations such as the home directory, `/usr/local`, and `/opt`. A command in a
core system directory such as `/usr/bin` is never offered as a raw file
deletion unless a supported package manager identifies how to remove it. This
also makes ownership-query failures and unsupported distribution managers fail
closed instead of risking system corruption.

Gear Lever installations are discovered through its machine-readable installed
app list, including custom AppImage folders. Removal is delegated back to Gear
Lever so the AppImage, desktop entry, icon, and Gear Lever update metadata are
handled together.

Software originally installed from a downloaded package file is found through
the system package database like any other package. You can also pass the
archive itself:

```sh
uninstall ~/Downloads/example.rpm
uninstall ./example.deb
uninstall ./example.pkg.tar.zst
```

`uninstall` reads only the archive metadata, verifies that the same package and
architecture are installed, and then uses the native package manager. If the
archive is older than the installed package, both versions are shown. An
unmatched archive is never mistaken for an installed app, and the downloaded
archive itself is not deleted.

On rpm-ostree systems, only explicitly layered package requests are ordinary
uninstall targets. Packages built into the base OS image are deliberately not
treated as layered packages or standalone executables. Changing the base image
is an advanced `rpm-ostree override remove` operation and is reported as such.

## Install

The installer downloads a self-contained, statically linked Rust executable for
the current architecture. It verifies the pinned release's published SHA-256
digest and exact version, stages it in the destination filesystem, and only
then atomically replaces the installed command. No Python runtime or Linux
distribution package is required. Release binaries cover x86-64, AArch64,
32-bit ARMv7, and 32-bit x86.

```sh
curl -fsSL https://raw.githubusercontent.com/JaredTweed/uninstall/main/install.sh | sh
```

The installer uses `sudo`, `doas`, or `pkexec` only when `/usr/local/bin` is not
writable.
Release executables also include keyless Sigstore bundles beside their checksum
files for independent verification with `cosign verify-blob`.
For a user-only installation:

```sh
mkdir -p "$HOME/.local/bin"
curl -fsSL https://raw.githubusercontent.com/JaredTweed/uninstall/main/install.sh |
  PREFIX="$HOME/.local" sh
```

Make sure `$HOME/.local/bin` is on your `PATH`.

## Use

```sh
uninstall FreeCAD
uninstall org.freecad.FreeCAD
uninstall ~/Downloads/example.rpm
uninstall --show-dependencies lib
uninstall FreeCAD --json
uninstall FreeCAD --debug
uninstall --help
uninstall uninstall
uninstall --self-uninstall
```

For automation, `--json` never changes state. Guarded non-interactive removal
requires an exact package ID, backend, and authorization phrase, and refuses
unknown or high-impact transactions:

```sh
uninstall ed --backend APT --confirm 'REMOVE APT:ed'
```

Matching is Unicode-aware and case-insensitive. A sole result is selected
automatically because the final transaction still requires confirmation. Very
short searches require an
exact match so queries such as `rg` do not produce hundreds of unrelated
results. Fuzzy dependency matches are collapsed when normal application
matches exist, but a dependency-only result is still shown. Exact matches and
command owners are never hidden. Use `--show-dependencies` to expand every
mixed result.

Each result has one concise `Why installed` line. For APT, DNF/YUM, Pacman,
Zypper, and Homebrew, `uninstall` traces dependencies back to an explicitly
installed application when the package database has enough information. On
DNF it also distinguishes user, group, hard-dependency, and weak-dependency
reasons and includes the original DNF5 transaction for automatically installed
packages when history is available:

```text
Why installed: wine recommends it; DNF transaction 193: dnf install wine (recorded reason: Weak Dependency; source repository: updates)
```

For packages installed through a DNF environment or group, cached installed
comps metadata supplies the immediate group and its display name while
transaction history supplies the original environment and command:

```text
Why installed: installed through COSMIC Desktop Environment → COSMIC Desktop Supplementary Applications (cosmic-desktop-apps); DNF transaction 224: dnf install @cosmic-desktop-environment
```

Group metadata is queried cache-only, so explaining an installation never
refreshes repositories or imports signing keys. If several groups could have
caused the installation, `uninstall` reports the generic group reason instead
of guessing. Exceptionally long retained commands are reduced to their
decisive install target and explicitly marked as abbreviated.

The same line uses the strongest locally recorded provenance available for
every supported backend:

- APT history (including rotated compressed logs), `apt-mark` state, and
  current dependency roots
- Pacman and Zypper history, explicit/dependency state, repositories, and
  current dependency roots
- legacy DNF/YUM transaction history, plain RPM database metadata, and
  rpm-ostree layered-package state
- Flatpak remote and install history; Snap channel, publisher, and retained
  snapd changes; Homebrew tap and install-receipt state
- Nix profile source attributes, Cargo registry or Git source, pipx package
  specification, and npm's retained resolved package URL
- Gear Lever update metadata and exact managed path, or an explicit statement
  that an AppImage or standalone executable has no known installation history
- APK's world constraints and dependency graph; OPKG's user/automatic status;
  XBPS manual-package and reverse-dependency metadata; and Portage's `@world`
  set and installed VDB
- Guix profile generations, legacy Nix environments, Slackware's installed
  package logs, Eopkg's retained package and reverse-dependency metadata, and
  Swupd's explicit bundle tracking and dependency metadata

Current state and historical evidence are worded separately. For example,
`marked manually installed by APT` describes current state, while an APT
history command describes a retained event. Missing history is stated as
unavailable rather than replaced with a guess. Likewise, passing a matching
`.rpm`, `.deb`, or Arch archive does not make it the claimed original source
unless retained history supports that conclusion.

Before asking for confirmation, native simulations determine what the package
manager expects to remove. A preview records whether the operation is exact,
blocked, a successful no-op, failed, unknown, or unsupported. Routine internal
details stay out of the way.
`uninstall` prints the information that can change the decision: installed
dependents, newly unused dependencies, protected or critical packages, or an
unavailable preview. High- and unknown-impact operations require typing the
exact confirmation phrase instead of answering `y`.
For one selected result, that phrase uses its displayed name (for example,
`REMOVE edit`) rather than a potentially long internal identifier or path.

When reliable metadata is available, the result line also shows the installed
application size. The final estimate includes selected application files,
additional packages in the native removal preview, package-manager cleanup,
and selected detected paths.

Running `uninstall` without an app prompts for one. The normal command always
explains the installation and checks removal impact.

When a backend supports multiple targets, selected results from that backend
are sent as one transaction—the same grouping used for its preview. Backends
that accept one target at a time and mixed backends remain separate commands.
User-data paths are deleted only if every command succeeds.

Search backends run concurrently in a bounded four-worker pool. Reusable APT,
RPM, Pacman, APK, OPKG, XBPS, and Eopkg inventories are loaded once per
invocation, and an exact command owner avoids a redundant full native-package
search. DNF role metadata is requested in one batch. These are invocation-only
caches: every new `uninstall` command reads the package managers' current state,
with no daemon or stale disk cache.

Dependency explanations are necessarily best effort: alternatives, rich
conditional dependencies, pruned transaction history, package groups, and
declarative systems do not always preserve the human reason an item was
installed. When metadata or a dry-run is unavailable the result says `UNKNOWN`;
it never silently claims the operation is safe. After cleanup choices, the
exact final command is previewed again. Its transaction is fingerprinted and
repeated immediately before execution; a change aborts instead of silently
accepting a different transaction. The executable is pinned before crossing a
privilege boundary, and installed state is queried again after execution.

APK, OPKG, XBPS, Portage, and Eopkg use their native no-action, dry-run, or
pretend operations. Portage removal uses dependency-aware `--depclean`, never
unsafe blind `--unmerge`; XBPS removal never forces reverse dependencies.
Slackware does not track dependency relationships. Eopkg can simulate removal
but does not provide stable machine-readable transaction output. Those cases
remain explicitly unknown-impact and require typed confirmation.
Swupd safely refuses to remove bundles needed by installed dependents, but has
no read-only removal transaction, so it also remains unknown-impact.

On read-only SUSE variants such as MicroOS, the final Zypper removal is routed
through `transactional-update` and takes effect in a new snapshot after reboot.

Nothing is removed during search or planning, invalid selections can be
retried, and pressing Enter at a prompt cancels.

Package-manager cleanup and detected user-data paths appear in one numbered
prompt:

```text
Remove associated data too? (optional)

   1. [Flatpak] Sandbox data and permissions       2.6 MiB
   2. [Detected] /home/jared/.cache/FreeCAD        0 B
   3. [Detected] /home/jared/.config/FreeCAD       28 KiB

Flatpak data is manager-owned. Detected paths are name matches, are not
guaranteed to belong to this app, and will be deleted permanently.
Choose numbers separated by commas, 'a' for all, or Enter to keep everything.
```

The available package-manager choice depends on the selected software:

- APT `purge` removes package-managed system configuration, not home data.
- Flatpak `--delete-data` removes sandbox data and permission-store entries.
- Snap `--purge` skips the automatic recovery snapshot; it does not delete
  snapshots that already exist.
- Eopkg `--purge` removes package-managed changed configuration files.
- Homebrew Cask `--zap` removes declared associated files and may include files
  shared by other apps.

When several backends are selected, each package-manager cleanup option gets
its own number.

Possible directories under the XDG config, cache, data, and state locations are
exact name or executable-path matches, not asserted ownership. They are
labelled `[Detected]` and selected individually. Every selected cleanup action
is shown under `Ready to run`, and detected paths are kept if any uninstall
command fails.

Sizes use binary units and represent the best available estimate of disk space
affected. Shared package objects, hard links, retained package caches, and
unreadable locations can make the final reclaimed space differ. The prompt
says `about`, `at least`, or `space estimate unavailable` as appropriate; an
unknown component is never silently treated as zero.

Do not run the whole program with `sudo`; `uninstall` invokes `sudo`, `doas`,
or `pkexec` itself only for selected system-wide operations. Shell built-ins
and shell functions are not installed executables and therefore cannot be
removed by this tool.

## Install from a fork

Point the installer at a release executable and its checksum:

```sh
curl -fsSL https://example.com/install.sh |
  UNINSTALL_SOURCE_URL=https://example.com/uninstall \
  UNINSTALL_CHECKSUM_URL=https://example.com/uninstall.sha256 sh
```

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

Unit and CLI tests use disposable files and never touch installed host
packages. CI additionally installs and removes a disposable package inside
isolated Debian, Ubuntu, Fedora, Alpine, Arch, openSUSE, and Void containers.
The same static release binary is used in every container.

## License

MIT
