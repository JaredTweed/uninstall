import importlib.util
import importlib.machinery
import sys
import tempfile
import unittest
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
        "freecad\t0.21\tExtensible Open Source CAx program\n"
        "firefox\t100\tWeb browser\n"
    ))
    def test_dpkg_can_match_description(self, _capture, _which):
        result = MODULE.detect_dpkg("cax")
        self.assertEqual([item.ident for item in result], ["freecad"])

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

    def test_selection_rejects_out_of_range_input(self):
        matches = [MODULE.Match("APT", "freecad", "freecad")]
        with patch("builtins.input", return_value="2"):
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


if __name__ == "__main__":
    unittest.main()
