# Backend capability matrix

“Exact preview” means the backend exposes a machine-readable or reliably parsed
read-only transaction. “Best effort” fails closed as unknown impact when output
cannot be interpreted.

| Backend | Discovery | Provenance | Dependency impact | Profiles/installations | Cleanup |
|---|---|---|---|---|---|
| APT/dpkg | Exact | History + current marks | Exact simulation | Multiarch IDs | Purge |
| DNF/DNF5/YUM | Exact | Transaction + groups | Exact parsed transaction | Architecture retained where available | Manager rules |
| RPM | Exact | Database metadata | Exact dependency test, no orphan plan | Architecture | None |
| rpm-ostree/bootc | Layered requests only | Deployment state | Exact dry run | Deployments | None |
| Zypper/transactional-update | Exact | History + current state | Exact XML dry run | Snapshots | Manager rules |
| URPMI/APT-RPM/microdnf | Exact RPM database | RPM metadata | Conservative native preview | Architecture | Manager rules |
| Pacman | Exact | Lifecycle history | Exact transaction print | Native database | None |
| APK | Exact | World + graph | Exact; blocked/no-op distinguished | World constraints | None |
| OPKG | Exact | Status metadata | Best effort no-action | Root database | None |
| XBPS | Exact | Manual state | Exact recursive dry run | Native database | None |
| Portage | Exact VDB | World + repository | Best effort depclean | Gentoo Prefix/slots | None |
| Slackware | Exact logs | Package logs | No dependency metadata | Native database | None |
| Eopkg | Exact | Automatic-parent metadata | Best effort dry run | Native database | Purge |
| Swupd | Exact bundles | Bundle tracking | Runtime refusal only | Upstream and third-party bundles | None |
| Flatpak | Exact apps | Remote + lifecycle history | Exact app target; related extensions noted | User, default system, named system | Delete data |
| Snap | Exact snaps | Channel + retained changes | Runtime snapd check | System | Purge/no snapshot |
| Gear Lever | Exact managed metadata | Update metadata | Manager lifecycle | Custom AppImage paths | Manager lifecycle |
| AppImage | Signature + desktop metadata | File path only | Exact file | User/system path | Detected paths |
| Homebrew/Cask | Exact JSON | Tap + requested state | Best effort | Active prefix | Cask zap |
| Nix/Guix | Exact profile entries | Source/profile metadata | Profile transaction | Named profiles | Store GC remains separate |
| Cargo/pipx/uv/npm | Exact top-level tools | Retained source where available | Exact target or fail-closed | User/system environment | Manager rules |
| Conda/Micromamba | Exact environment inventory | Channel + prefix | Exact JSON dry run | Named prefixes | Manager rules |
| Standalone | Exact command path | Unknown by definition | No dependency metadata | Safe application locations | Exact opt-in matches |
| Distrobox/Toolbox export | Exact desktop export | Container name | Host removal blocked | Container identity | Remove inside container |

Clear Linux Swupd and unstructured legacy-manager output are compatibility
targets. They remain deliberately conservative because their upstream metadata
cannot always support an exact preview.
