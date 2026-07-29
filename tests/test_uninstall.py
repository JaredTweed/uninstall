import importlib.util
import importlib.machinery
import sys
import tempfile
import unittest
import os
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).parents[1] / "uninstall"
LOADER = importlib.machinery.SourceFileLoader("uninstall_cli", str(SCRIPT))
SPEC = importlib.util.spec_from_loader("uninstall_cli", LOADER)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class UninstallTests(unittest.TestCase):
    def test_matching_ignores_case_and_punctuation(self):
        self.assertTrue(MODULE.relevant("FreeCAD", "org.freecad.FreeCAD"))
        self.assertTrue(MODULE.relevant("visual studio", "visual-studio-code"))
        self.assertFalse(MODULE.relevant("freecad", "Firefox"))
        self.assertTrue(MODULE.relevant("éditeur", "Éditeur"))
        self.assertFalse(MODULE.relevant("rg", "org.mozilla.firefox"))
        self.assertTrue(MODULE.relevant("rg", "rg"))

    def test_terminal_control_characters_are_neutralized(self):
        self.assertEqual(MODULE.display("safe\x1b[31m\nname"), "safe?[31m?name")

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/flatpak")
    @patch.object(MODULE, "capture")
    def test_flatpak_detects_user_and_system_scope(self, capture, _which):
        capture.side_effect = [
            "org.freecad.FreeCAD\tFreeCAD\t1.0\n",
            "org.mozilla.firefox\tFirefox\t2.0\n",
        ]
        result = MODULE.detect_flatpak("freecad")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].ident, "org.freecad.FreeCAD")
        self.assertEqual(result[0].scope, "user")

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dpkg-query")
    @patch.object(MODULE, "capture", return_value=(
        "ii \tfreecad\t0.21\tExtensible Open Source CAx program\n"
        "rc \told-freecad\t0.19\tRemoved package configuration\n"
        "ii \tfirefox\t100\tWeb browser\n"
    ))
    def test_dpkg_can_match_description(self, _capture, _which):
        result = MODULE.detect_dpkg("cax")
        self.assertEqual([item.ident for item in result], ["freecad"])

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_rpm_finds_package_owning_differently_named_command(
            self, capture, which):
        locations = {
            "aafire": "/usr/bin/aafire",
            "rpm": "/usr/bin/rpm",
            "dnf5": "/usr/bin/dnf5",
        }
        which.side_effect = locations.get
        capture.return_value = "aalib\t1.4.0-0.58.rc5.fc44\n"
        result = MODULE.detect_executable_owner("aafire")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "DNF")
        self.assertEqual(result[0].ident, "aalib")
        self.assertEqual(result[0].provides, "/usr/bin/aafire")
        capture.assert_called_once_with([
            "rpm", "-qf", "--qf", "%{NAME}\\t%{VERSION}-%{RELEASE}\\n",
            "/usr/bin/aafire",
        ])

    @patch.object(MODULE.shutil, "which", return_value=None)
    def test_missing_command_has_no_owner(self, _which):
        self.assertEqual(MODULE.detect_executable_owner("not-a-command"), [])

    @patch.object(MODULE.Path, "home", return_value=Path("/home/test"))
    @patch.object(MODULE.os, "access", return_value=True)
    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture", return_value="")
    def test_unowned_command_is_standalone(
            self, _capture, which, _access, _home):
        locations = {
            "edit": "/home/test/.local/bin/edit",
            "rpm": "/usr/bin/rpm",
            "dnf5": "/usr/bin/dnf5",
        }
        which.side_effect = locations.get
        result = MODULE.detect_executable_owner("edit")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "Standalone")
        self.assertEqual(result[0].path, Path("/home/test/.local/bin/edit"))
        self.assertEqual(
            MODULE.uninstall_command(result[0], False),
            ["rm", "--", "/home/test/.local/bin/edit"],
        )

    @patch.object(MODULE, "DETECTORS")
    def test_exact_command_suppresses_loose_package_matches(self, detectors):
        detectors.__iter__.return_value = iter([
            lambda _query: [
                MODULE.Match("Standalone", "/home/test/.local/bin/edit",
                             "edit", scope="user",
                             path=Path("/home/test/.local/bin/edit")),
                MODULE.Match("DNF", "libedit", "libedit", scope="system"),
                MODULE.Match("Flatpak", "com.example.edit", "Edit",
                             scope="user"),
            ],
        ])
        result = MODULE.find_matches("edit")
        self.assertEqual(
            [(item.kind, item.name) for item in result],
            [("Standalone", "edit"), ("Flatpak", "Edit")],
        )

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_snap_alias_is_not_mistaken_for_standalone(self, capture, which):
        which.side_effect = {
            "lxc": "/snap/bin/lxc",
            "snap": "/usr/bin/snap",
        }.get
        capture.side_effect = [
            "Command Alias Notes\nlxd.lxc lxc -\n",
            "Name Version Rev Tracking Publisher Notes\nlxd 5.0 1 latest x -\n",
        ]
        result = MODULE.detect_executable_owner("lxc")
        self.assertEqual([(item.kind, item.ident) for item in result], [("Snap", "lxd")])

    @patch.object(MODULE.Path, "home", return_value=Path("/home/test"))
    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_pipx_command_uses_pipx_uninstaller(self, capture, which, _home):
        which.side_effect = {
            "black": "/home/test/.local/bin/black",
            "pipx": "/usr/bin/pipx",
        }.get
        with patch.object(
                MODULE.Path, "resolve",
                return_value=Path("/home/test/.local/share/pipx/venvs/black/bin/black")):
            result = MODULE.detect_executable_owner("black")
        self.assertEqual(result[0].kind, "Pipx")
        self.assertEqual(
            MODULE.uninstall_command(result[0], False),
            ["pipx", "uninstall", "black"],
        )
        capture.assert_not_called()

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture", return_value="/home/test/.local\n")
    def test_nested_npm_dependency_is_not_treated_as_global_app(
            self, _capture, which):
        nested = (
            "/home/test/.local/lib/node_modules/parent/node_modules/"
            "child/vendor/tool"
        )
        which.side_effect = {"tool": nested, "npm": "/usr/bin/npm"}.get
        self.assertEqual(MODULE.detect_executable_owner("tool"), [])

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_exposed_global_npm_command_uses_npm_uninstaller(
            self, capture, which):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory)
            target = prefix / "lib/node_modules/@scope/tool/bin/tool"
            target.parent.mkdir(parents=True)
            target.touch()
            (prefix / "bin").mkdir()
            link = prefix / "bin/tool"
            link.symlink_to(target)
            which.side_effect = {
                "tool": str(link),
                "npm": "/usr/bin/npm",
            }.get
            capture.return_value = str(prefix) + "\n"
            result = MODULE.detect_executable_owner("tool")
        self.assertEqual((result[0].kind, result[0].ident), ("NPM", "@scope/tool"))
        with patch.object(MODULE.os, "geteuid", return_value=0):
            self.assertEqual(
                MODULE.uninstall_command(result[0], False),
                ["npm", "uninstall", "--global", "@scope/tool"],
            )

    @patch.object(MODULE.Path, "home", return_value=Path("/home/test"))
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/npm")
    @patch.object(MODULE, "capture")
    def test_npm_packages_can_be_found_by_package_name(
            self, capture, _which, _home):
        capture.side_effect = [
            "/home/test/.local\n",
            '{"dependencies":{"@scope/tool":{"version":"2.1.0"}}}\n',
        ]
        result = MODULE.detect_npm("@scope/tool")
        self.assertEqual(
            (result[0].kind, result[0].ident, result[0].version, result[0].scope),
            ("NPM", "@scope/tool", "2.1.0", "user"),
        )

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/pipx")
    @patch.object(MODULE, "capture")
    def test_pipx_packages_can_be_found_without_knowing_exposed_command(
            self, capture, _which):
        capture.side_effect = ["httpie 3.2.4\n", ""]
        result = MODULE.detect_pipx("httpie")
        self.assertEqual(
            (result[0].kind, result[0].ident, result[0].version),
            ("Pipx", "httpie", "3.2.4"),
        )

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture", return_value=(
        "ripgrep v14.1.1:\n"
        "    rg\n"
        "bat v0.25.0:\n"
        "    bat\n"
    ))
    def test_cargo_maps_binary_to_crate(self, _capture, which):
        which.side_effect = {
            "cargo": "/usr/bin/cargo",
            "rg": "/home/test/.cargo/bin/rg",
        }.get
        with patch.dict(os.environ, {"CARGO_HOME": "/home/test/.cargo"}):
            result = MODULE.detect_cargo("rg")
        self.assertEqual((result[0].kind, result[0].ident), ("Cargo", "ripgrep"))
        self.assertEqual(result[0].provides, "/home/test/.cargo/bin/rg")

    def test_nix_profile_parser_handles_named_and_indexed_formats(self):
        text = (
            "Name: hello\nStore paths: /nix/store/abc-hello\n\n"
            "Index: 2\nStore paths: /nix/store/def-tool /nix/store/ghi-lib\n"
        )
        self.assertEqual(
            MODULE.parse_nix_profile(text),
            [("hello", ["/nix/store/abc-hello"]),
             ("2", ["/nix/store/def-tool", "/nix/store/ghi-lib"])],
        )

    def test_commands_are_exact_and_do_not_use_a_shell(self):
        item = MODULE.Match("Flatpak", "org.freecad.FreeCAD", "FreeCAD",
                            "1.0", "user")
        self.assertEqual(
            MODULE.uninstall_command(item, True),
            ["flatpak", "uninstall", "-y", "--user", "--delete-data",
             "org.freecad.FreeCAD"],
        )

    def test_user_data_only_matches_exact_directory_name(self):
        selected = [MODULE.Match("APT", "freecad", "freecad")]
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            (home / ".config").mkdir()
            exact = home / ".config/freecad"
            similar = home / ".config/freecad-backup"
            exact.mkdir()
            similar.mkdir()
            with patch.object(MODULE.Path, "home", return_value=home):
                result = MODULE.find_user_data(selected)
            self.assertEqual(result, [exact])

    def test_appimage_version_is_removed_when_looking_for_app_data(self):
        selected = [
            MODULE.Match("AppImage", "/tmp/FreeCAD-1.2.3.AppImage",
                         "FreeCAD-1.2.3")
        ]
        self.assertIn("freecad", MODULE.data_keys(selected))

    def test_appimage_desktop_entry_is_offered_for_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            data_home = Path(directory)
            applications = data_home / "applications"
            applications.mkdir()
            desktop = applications / "custom-name.desktop"
            desktop.write_text(
                "[Desktop Entry]\nExec=/tmp/FreeCAD-1.2.3.AppImage\n",
                encoding="utf-8",
            )
            selected = [
                MODULE.Match(
                    "AppImage", "/tmp/FreeCAD-1.2.3.AppImage",
                    "FreeCAD-1.2.3", path=Path("/tmp/FreeCAD-1.2.3.AppImage"))
            ]
            with patch.dict(os.environ, {"XDG_DATA_HOME": str(data_home)}):
                self.assertIn(desktop, MODULE.find_user_data(selected))

    def test_custom_xdg_data_location_is_found_and_allowed(self):
        selected = [MODULE.Match("Standalone", "/tmp/tool", "tool")]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "configuration"
            root.mkdir()
            candidate = root / "tool"
            candidate.mkdir()
            env = {"XDG_CONFIG_HOME": str(root)}
            with patch.dict(os.environ, env, clear=False):
                self.assertIn(candidate, MODULE.find_user_data(selected))
                self.assertTrue(MODULE.remove_paths([candidate]))
            self.assertFalse(candidate.exists())

    def test_preflight_rejects_command_that_changed_path(self):
        item = MODULE.Match(
            "Standalone", "/tmp/edit", "edit", path=Path("/tmp/edit"))
        with patch.object(MODULE.Path, "exists", return_value=True), \
                patch.object(MODULE.Path, "is_dir", return_value=False), \
                patch.object(MODULE.shutil, "which", return_value="/tmp/other-edit"):
            self.assertIn("different path", MODULE.preflight_file(item))

    @patch.object(MODULE, "remove_paths")
    @patch.object(MODULE.subprocess, "run")
    @patch.object(MODULE, "find_user_data")
    @patch.object(MODULE, "find_matches")
    def test_failed_uninstall_never_deletes_selected_app_data(
            self, find_matches, find_user_data, run, remove_paths):
        item = MODULE.Match("Flatpak", "org.example.App", "Example", scope="user")
        find_matches.return_value = [item]
        find_user_data.return_value = [Path("/home/test/.config/Example")]
        run.return_value.returncode = 1
        with patch("builtins.input", side_effect=["1", "y", "y"]):
            self.assertEqual(MODULE.run_uninstall("Example"), 1)
        remove_paths.assert_not_called()

    @patch.object(MODULE.subprocess, "run")
    @patch.object(MODULE, "find_user_data", return_value=[])
    @patch.object(MODULE, "find_matches")
    def test_cancel_at_final_confirmation_runs_nothing(
            self, find_matches, _find_user_data, run):
        find_matches.return_value = [
            MODULE.Match("Cargo", "tool", "tool", scope="user")
        ]
        with patch("builtins.input", side_effect=["1", ""]):
            self.assertEqual(MODULE.run_uninstall("tool"), 0)
        run.assert_not_called()

    @patch.object(MODULE.os, "geteuid", return_value=1000)
    @patch.object(MODULE.shutil, "which")
    def test_doas_is_used_when_sudo_is_unavailable(self, which, _euid):
        which.side_effect = {"doas": "/usr/bin/doas"}.get
        self.assertEqual(
            MODULE.privileged(["dnf", "remove", "thing"]),
            ["doas", "dnf", "remove", "thing"],
        )

    @patch.object(MODULE.os, "geteuid", return_value=1000)
    @patch.object(MODULE.shutil, "which", return_value=None)
    def test_missing_privilege_helper_fails_before_removal(self, _which, _euid):
        with self.assertRaisesRegex(RuntimeError, "neither sudo nor doas"):
            MODULE.privileged(["dnf", "remove", "thing"])

    def test_selection_rejects_out_of_range_input(self):
        matches = [MODULE.Match("APT", "freecad", "freecad")]
        with patch("builtins.input", side_effect=["2", ""]):
            self.assertEqual(MODULE.choose(matches), [])

    @patch.object(MODULE, "self_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall"])
    def test_uninstall_uninstall_is_self_uninstall(self, self_uninstall):
        self.assertEqual(MODULE.main(), 0)
        self_uninstall.assert_called_once_with()

    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall-helper"])
    def test_longer_name_remains_a_normal_search(self, run_uninstall):
        self.assertEqual(MODULE.main(), 0)
        run_uninstall.assert_called_once_with("uninstall-helper")

    @patch.object(MODULE.os, "geteuid", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "freecad"])
    def test_running_whole_program_through_sudo_is_refused(self, _euid):
        with patch.dict(os.environ, {"SUDO_USER": "test"}, clear=False):
            self.assertEqual(MODULE.main(), 2)


if __name__ == "__main__":
    unittest.main()
