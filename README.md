# uninstall

`uninstall` answers a surprisingly awkward Linux question: “How did I install
this app or command, and how do I remove it?”

```console
$ uninstall FreeCAD
Searching installed apps for “FreeCAD”…

Found 2 likely installed options:

   1. FreeCAD  [Flatpak]  0.21.2 · user
      org.freecad.FreeCAD
   2. freecad  [APT]  0.20.2 · system

Choose numbers separated by commas, “a” for all, or Enter to cancel.
> 1
```

It searches:

- Flatpak, Snap, AppImage, and AppImages managed by Gear Lever
- APT/dpkg, DNF/YUM/RPM, rpm-ostree, Zypper/RPM, and Pacman
- Homebrew, Nix profiles, Cargo, pipx, and global npm packages
- standalone executables on `PATH`

You choose the exact result, decide separately whether its data should go too,
see the command that will run, and confirm once more. If a command has a
different package name—such as `aafire` from `aalib`—it asks the system package
database which package owns the executable. Manager shims, aliases, symlinks,
and alternatives are resolved before falling back to standalone removal.

If no manager owns a command, `uninstall` can remove its exact executable from
`PATH`. It labels this as `Standalone` because files installed alongside that
executable cannot always be inferred safely.

Gear Lever installations are discovered through its machine-readable installed
app list, including custom AppImage folders. Removal is delegated back to Gear
Lever so the AppImage, desktop entry, icon, and Gear Lever update metadata are
handled together.

## Install

The installer checks for Python 3.8+, `curl`, and the standard system utilities it
needs before changing anything. It downloads the matching pinned release to a
temporary file, verifies the CLI can start, and only then installs it.

```sh
curl -fsSL https://raw.githubusercontent.com/JaredTweed/uninstall/main/install.sh | sh
```

The installer uses `sudo` only when `/usr/local/bin` is not writable. For a
user-only installation:

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
uninstall --why aalib-libs
uninstall --plan wine
uninstall --show-dependencies lib
uninstall --help
uninstall uninstall
uninstall --self-uninstall
```

Matching is Unicode-aware and case-insensitive. Very short searches require an
exact match so queries such as `rg` do not produce hundreds of unrelated
results. Fuzzy matches that the native manager records as automatic
dependencies are collapsed by default; exact matches and command owners are
never hidden. Use `--show-dependencies` to expand them.

Before any app-data question or removal, `uninstall` builds a read-only
dependency and transaction plan. For APT, DNF/RPM, Pacman, and Homebrew it
reports direct dependents and traces best-effort paths back to explicitly
installed root applications:

```text
libfoo [DNF]
  Installation role: dependency
  Directly required by: wine-core
  Root causes:
    wine → wine-core → libfoo
```

Native simulations determine the packages actually scheduled for removal.
Impact is labelled `SAFE`, `CAUTION`, `HIGH`, or `UNKNOWN`. High- and
unknown-impact operations require typing the exact confirmation phrase rather
than answering `y`. Essential, held, protected, and core packages receive an
additional warning. `--why` explains dependency paths without creating a
transaction; `--plan` includes the native transaction preview and then exits.

Dependency explanations are necessarily best effort: alternatives, weak
dependencies, package groups, and declarative systems do not always preserve
the human reason an item was installed. When metadata or a dry-run is
unavailable the result says `UNKNOWN`; it never silently claims the operation
is safe. The package manager's final transaction is always authoritative.

Nothing is removed during search or planning, invalid selections can be
retried, and pressing Enter at a prompt cancels.

APT's app-data option uses `purge` for packaged configuration; Flatpak uses
`--delete-data`; Snap uses `--purge`. Exact matching directories under the XDG
config, cache, data, and state locations are shown before deletion. Data is
kept if any uninstall command fails.

Do not run the whole program with `sudo`; `uninstall` invokes `sudo` or `doas`
itself only for selected system-wide operations. Shell built-ins and shell
functions are not installed executables and therefore cannot be removed by
this tool.

## Install from a fork

Point the installer at the raw executable:

```sh
curl -fsSL https://example.com/install.sh |
  UNINSTALL_SOURCE_URL=https://example.com/uninstall sh
```

## Development

```sh
python3 -m unittest discover -s tests -v
```

The test suite replaces package managers with harmless fixtures and never
uninstalls real packages.

## License

MIT
