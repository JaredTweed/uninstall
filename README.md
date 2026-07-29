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

It searches Flatpak, Snap, APT/dpkg, DNF/YUM/RPM, Zypper/RPM, Pacman, and
common AppImage locations. You choose the exact result, decide separately
whether its data should go too, see the command that will run, and confirm once
more. If a command has a different package name—such as `aafire` from
`aalib`—it asks the system package database which package owns the executable.

## Install

Python 3 and `curl` are the only requirements.

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
uninstall --help
uninstall uninstall
uninstall --self-uninstall
```

Matching is case-insensitive. Nothing is removed during search, and pressing
Enter at either prompt cancels. APT's app-data option uses `purge` for packaged
configuration; Flatpak uses `--delete-data`; Snap uses `--purge`. Exact matching
directories under `~/.config`, `~/.cache`, `~/.local/share`, and
`~/.local/state` are shown before deletion.

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
