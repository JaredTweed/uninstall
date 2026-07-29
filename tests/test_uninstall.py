import importlib.util
import importlib.machinery
import io
import subprocess
import sys
import tempfile
import unittest
import os
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import call, patch


SCRIPT = Path(__file__).parents[1] / "uninstall"
INSTALLER = Path(__file__).parents[1] / "install.sh"
LOADER = importlib.machinery.SourceFileLoader("uninstall_cli", str(SCRIPT))
SPEC = importlib.util.spec_from_loader("uninstall_cli", LOADER)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class UninstallTests(unittest.TestCase):
    def setUp(self):
        MODULE.rpm_ostree_layered_packages.cache_clear()
        MODULE.zypper_userinstalled.cache_clear()
        MODULE.dnf_install_reasons.cache_clear()
        MODULE.dnf_history_reason.cache_clear()
        MODULE.rpm_dependency_graph.cache_clear()
        MODULE.rpm_reverse_graph.cache_clear()
        MODULE.apt_reverse_graph.cache_clear()
        MODULE.pacman_reverse_graph.cache_clear()
        MODULE.explicit_names_for_kind.cache_clear()
        MODULE.apt_held_packages.cache_clear()
        MODULE.dnf_protected_patterns.cache_clear()

    def test_installer_creates_a_completely_new_custom_prefix(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "new" / "prefix"
            environment = os.environ.copy()
            environment.update({
                "PREFIX": str(prefix),
                "UNINSTALL_SOURCE_URL": SCRIPT.as_uri(),
            })
            result = subprocess.run(
                ["sh", str(INSTALLER)],
                check=False,
                capture_output=True,
                text=True,
                env=environment,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            installed = prefix / "bin" / "uninstall"
            self.assertTrue(installed.is_file())
            self.assertTrue(os.access(installed, os.X_OK))
            version = subprocess.run(
                [str(installed), "--version"],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(version.returncode, 0, version.stderr)
            self.assertIn(MODULE.VERSION, version.stdout)

    def test_installer_rejects_a_source_with_the_wrong_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "wrong-version"
            source.write_text(
                SCRIPT.read_text(encoding="utf-8").replace(
                    f'VERSION = "{MODULE.VERSION}"',
                    'VERSION = "999.0.0"',
                    1,
                ),
                encoding="utf-8",
            )
            source.chmod(0o755)
            environment = os.environ.copy()
            environment.update({
                "PREFIX": str(root / "prefix"),
                "UNINSTALL_SOURCE_URL": source.as_uri(),
            })
            result = subprocess.run(
                ["sh", str(INSTALLER)],
                check=False,
                capture_output=True,
                text=True,
                env=environment,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("version mismatch", result.stderr)

    def test_matching_ignores_case_and_punctuation(self):
        self.assertTrue(MODULE.relevant("FreeCAD", "org.freecad.FreeCAD"))
        self.assertTrue(MODULE.relevant("visual studio", "visual-studio-code"))
        self.assertFalse(MODULE.relevant("freecad", "Firefox"))
        self.assertTrue(MODULE.relevant("éditeur", "Éditeur"))
        self.assertFalse(MODULE.relevant("rg", "org.mozilla.firefox"))
        self.assertTrue(MODULE.relevant("rg", "rg"))

    def test_terminal_control_characters_are_neutralized(self):
        self.assertEqual(MODULE.display("safe\x1b[31m\nname"), "safe?[31m?name")

    def test_ascii_locale_does_not_crash_on_friendly_help_text(self):
        environment = os.environ.copy()
        environment.update({"LC_ALL": "C", "PYTHONUTF8": "0"})
        result = subprocess.run(
            [str(SCRIPT), "--help"],
            check=False,
            capture_output=True,
            env=environment,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(b"--why", result.stdout)

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

    def test_gearlever_json_is_parsed_without_table_guessing(self):
        result = MODULE.parse_gearlever_list(
            'log line\n{"schema_version": 1, "installed": ['
            '{"name": "Arduino IDE", "path": "/home/test/AppImages/arduino.appimage",'
            ' "current_version": "2.3.6", "desktop_id": "arduino.desktop"}]}\n'
        )
        self.assertEqual(result, [{
            "name": "Arduino IDE",
            "path": "/home/test/AppImages/arduino.appimage",
            "current_version": "2.3.6",
            "desktop_id": "arduino.desktop",
        }])

    def test_gearlever_older_table_output_supports_spaces_in_paths(self):
        result = MODULE.parse_gearlever_list(
            "My App   [Not specified]   [UpdatesNotAvailable]   "
            "/home/test/My App.AppImage   \n"
        )
        self.assertEqual(result, [{
            "name": "My App",
            "path": "/home/test/My App.AppImage",
            "current_version": "",
        }])

    @patch.object(MODULE.shutil, "which", return_value=None)
    @patch.object(MODULE, "capture")
    @patch.object(
        MODULE, "gearlever_command",
        return_value=["flatpak", "run", "it.mijorus.gearlever"],
    )
    def test_gearlever_apps_are_detected_from_managed_metadata(
            self, _command, capture, _which):
        with tempfile.TemporaryDirectory() as directory:
            appimage = Path(directory) / "arduino_ide.appimage"
            appimage.touch()
            capture.return_value = MODULE.json.dumps({
                "schema_version": 1,
                "installed": [{
                    "name": "Arduino IDE",
                    "path": str(appimage),
                    "current_version": "2.3.6",
                    "desktop_id": "arduino_ide.desktop",
                }],
            })
            result = MODULE.detect_gearlever("arduino")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "Gear Lever")
        self.assertEqual(result[0].name, "Arduino IDE")
        self.assertEqual(result[0].version, "2.3.6")
        self.assertEqual(result[0].path, appimage)
        capture.assert_called_once_with([
            "flatpak", "run", "it.mijorus.gearlever",
            "--list-installed", "--json",
        ])

    @patch.object(
        MODULE, "gearlever_command",
        return_value=["flatpak", "run", "it.mijorus.gearlever"],
    )
    def test_gearlever_removal_uses_its_complete_lifecycle(self, _command):
        path = Path("/home/test/AppImages/arduino_ide.appimage")
        item = MODULE.Match(
            "Gear Lever", str(path), "Arduino IDE", path=path)
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            [
                "flatpak", "run", "it.mijorus.gearlever",
                "--remove", str(path), "--yes",
            ],
        )

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_rpm_archive_resolves_to_currently_installed_version(
            self, capture, which):
        which.side_effect = {
            "rpm": "/usr/bin/rpm",
            "dnf5": "/usr/bin/dnf5",
        }.get
        capture.side_effect = [
            "example\t1.0-1\tx86_64\n",
            "example\t2.0-3\tx86_64\n",
        ]
        with tempfile.NamedTemporaryFile(suffix=".rpm") as archive:
            result = MODULE.detect_package_archive(archive.name)
            archive_path = Path(archive.name).absolute()
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "DNF")
        self.assertEqual(result[0].ident, "example")
        self.assertEqual(result[0].version, "2.0-3")
        self.assertEqual(result[0].archive_version, "1.0-1")
        self.assertEqual(result[0].archive, archive_path)
        self.assertEqual(capture.call_args_list, [
            call([
                "rpm", "-qp", "--qf",
                "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{ARCH}\\n",
                "--", str(archive_path),
            ]),
            call([
                "rpm", "-q", "--qf",
                "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{ARCH}\\n",
                "--", "example.x86_64",
            ]),
        ])

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_deb_archive_resolves_multiarch_installed_package(
            self, capture, which):
        which.side_effect = {
            "dpkg-deb": "/usr/bin/dpkg-deb",
            "dpkg-query": "/usr/bin/dpkg-query",
        }.get
        capture.side_effect = [
            "example\t1.0-1\tamd64\n",
            "ii \texample:amd64\t1.1-2\n",
        ]
        with tempfile.NamedTemporaryFile(suffix=".deb") as archive:
            result = MODULE.detect_package_archive(archive.name)
            archive_path = Path(archive.name).absolute()
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "APT")
        self.assertEqual(result[0].ident, "example:amd64")
        self.assertEqual(result[0].version, "1.1-2")
        self.assertEqual(result[0].archive_version, "1.0-1")
        self.assertEqual(capture.call_args_list, [
            call([
                "dpkg-deb", "--show",
                "--showformat=${Package}\\t${Version}\\t${Architecture}\\n",
                str(archive_path),
            ]),
            call([
                "dpkg-query", "-W",
                "-f=${db:Status-Abbrev}\\t${binary:Package}\\t${Version}\\n",
                "example:amd64",
            ]),
        ])

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/pacman")
    @patch.object(MODULE, "capture")
    def test_arch_archive_resolves_installed_package(
            self, capture, _which):
        capture.side_effect = [
            "example 1.0-1\n",
            "example 1.1-1\n",
        ]
        with tempfile.NamedTemporaryFile(suffix=".pkg.tar.zst") as archive:
            result = MODULE.detect_package_archive(archive.name)
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].kind, "Pacman")
        self.assertEqual(result[0].version, "1.1-1")
        self.assertEqual(result[0].archive_version, "1.0-1")

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

    @patch.object(MODULE.Path, "home", return_value=Path("/home/test"))
    @patch.object(MODULE.shutil, "which")
    def test_unowned_system_command_is_never_treated_as_standalone(
            self, which, _home):
        which.side_effect = {
            "mystery": "/usr/bin/mystery",
        }.get
        self.assertEqual(
            MODULE.detect_executable_owner("mystery"),
            [],
        )
        self.assertEqual(
            MODULE.unidentified_system_command("mystery"),
            "/usr/bin/mystery",
        )

    @patch.object(MODULE.shutil, "which")
    def test_extensionless_appimage_command_removes_image_and_symlink(
            self, which):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "Applications" / "editor"
            image.parent.mkdir()
            image.write_bytes(
                b"\x7fELF" + b"\0" * 4 + b"AI\x02" + b"\0" * 8)
            image.chmod(0o755)
            link = root / "bin" / "edit"
            link.parent.mkdir()
            link.symlink_to(image)
            which.side_effect = {
                "edit": str(link),
            }.get
            with patch.object(MODULE.Path, "home", return_value=root):
                result = MODULE.detect_executable_owner("edit")
            self.assertEqual(len(result), 1)
            self.assertEqual(result[0].kind, "AppImage")
            self.assertEqual(result[0].path, image)
            self.assertEqual(result[0].provides, str(link))
            self.assertEqual(
                MODULE.uninstall_command(result[0], False),
                ["rm", "--", str(image), str(link)],
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

    @patch.object(MODULE, "DETECTORS")
    def test_gearlever_metadata_wins_over_generic_appimage(self, detectors):
        path = Path("/home/test/Applications/example.appimage")
        managed = MODULE.Match(
            "Gear Lever", str(path), "Example", path=path)
        generic = MODULE.Match(
            "AppImage", str(path), "example", path=path)
        detectors.__iter__.return_value = iter([
            lambda _query: [generic, managed],
        ])
        self.assertEqual(MODULE.find_matches(str(path)), [managed])

    @patch.object(MODULE, "DETECTORS")
    def test_package_owner_wins_over_generic_appimage(self, detectors):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image_path = root / "edit"
            image_path.touch()
            command_path = root / "bin" / "edit"
            command_path.parent.mkdir()
            command_path.symlink_to(image_path)
            managed = MODULE.Match(
                "DNF", "packaged-editor", "packaged-editor",
                provides=str(command_path),
            )
            appimage = MODULE.Match(
                "AppImage", str(image_path), "edit", path=image_path)
            detectors.__iter__.return_value = iter([
                lambda _query: [appimage, managed],
            ])
            self.assertEqual(MODULE.find_matches("edit"), [managed])

    @patch.object(MODULE, "capture_any")
    @patch.object(MODULE.shutil, "which")
    def test_apt_roles_distinguish_explicit_from_dependency(
            self, which, capture_any):
        which.side_effect = {"apt-mark": "/usr/bin/apt-mark"}.get
        capture_any.return_value = (0, "libfoo\n")
        matches = [
            MODULE.Match("APT", "wine", "wine"),
            MODULE.Match("APT", "libfoo:amd64", "libfoo:amd64"),
        ]
        result = MODULE.annotate_roles(matches)
        self.assertEqual([item.role for item in result], ["explicit", "dependency"])

    def test_fuzzy_dependencies_are_collapsed_but_exact_ones_are_not(self):
        dependency = MODULE.Match(
            "DNF", "libedit", "libedit", role="dependency")
        application = MODULE.Match(
            "DNF", "cosmic-edit", "cosmic-edit", role="explicit")
        visible, hidden = MODULE.filter_dependency_matches(
            [dependency, application], "edit", False)
        self.assertEqual((visible, hidden), ([application], 1))
        visible, hidden = MODULE.filter_dependency_matches(
            [dependency], "libedit", False)
        self.assertEqual((visible, hidden), ([dependency], 0))
        visible, hidden = MODULE.filter_dependency_matches(
            [dependency], "edit", False)
        self.assertEqual((visible, hidden), ([dependency], 0))

    @patch.object(MODULE, "capture_any")
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_dnf_role_preserves_weak_dependency_reason(
            self, _which, capture_any):
        capture_any.return_value = (
            0, "dosbox-staging|Weak Dependency\nwine|User\n")
        matches = [
            MODULE.Match("DNF", "dosbox-staging", "dosbox-staging"),
            MODULE.Match("DNF", "wine", "wine"),
        ]
        result = MODULE.annotate_roles(matches)
        self.assertEqual(
            [item.role for item in result],
            ["weak dependency", "explicit"],
        )

    @patch.object(MODULE, "capture_any")
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/zypper")
    def test_zypper_roles_use_machine_readable_install_reasons(
            self, _which, capture_any):
        capture_any.return_value = (
            0,
            '<?xml version="1.0"?><stream><package-list>'
            '<solvable kind="package" name="editor"/>'
            '</package-list></stream>',
        )
        matches = [
            MODULE.Match("Zypper", "editor", "editor"),
            MODULE.Match("Zypper", "libeditor", "libeditor"),
        ]
        result = MODULE.annotate_roles(matches)
        self.assertEqual(
            [item.role for item in result],
            ["explicit", "dependency"],
        )

    @patch.object(MODULE, "capture")
    @patch.object(MODULE.shutil, "which")
    def test_rpm_ostree_inventory_contains_only_layered_requests(
            self, which, capture):
        which.side_effect = {
            "rpm-ostree": "/usr/bin/rpm-ostree",
        }.get
        capture.return_value = MODULE.json.dumps({
            "deployments": [{
                "booted": True,
                "requested-packages": ["layered-app", "inactive-request"],
                "packages": ["layered-app"],
            }],
        })
        ok, names = MODULE.rpm_ostree_layered_packages()
        self.assertTrue(ok)
        self.assertEqual(names, {"layered-app", "inactive-request"})

    @patch.object(MODULE, "rpm_manager", return_value="RPM-OSTree")
    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_rpm_ostree_base_command_is_not_mistaken_for_standalone(
            self, capture, which, _manager):
        which.side_effect = {
            "base-tool": "/usr/bin/base-tool",
            "rpm": "/usr/bin/rpm",
            "rpm-ostree": "/usr/bin/rpm-ostree",
        }.get
        capture.side_effect = [
            "base-package\t1.0-1\n",
            MODULE.json.dumps({
                "deployments": [{
                    "booted": True,
                    "requested-packages": [],
                    "packages": [],
                }],
            }),
        ]
        self.assertEqual(
            MODULE.detect_executable_owner("base-tool"),
            [],
        )

    @patch.object(MODULE, "capture")
    def test_rpm_capability_graph_finds_real_reverse_dependencies(self, capture):
        capture.return_value = (
            "P\tlibfoo\nS\tlibfoo\nS\tlibfoo.so.1()(64bit)\n"
            "P\twine-core\nR\tlibfoo.so.1()(64bit)\nS\twine-core\n"
            "P\twine\nR\twine-core\nW\tdosbox-staging\nS\twine\n"
            "P\tdosbox-staging\nS\tdosbox-staging\n"
        )
        reverse, complete = MODULE.rpm_reverse_graph()
        self.assertTrue(complete)
        self.assertEqual(reverse["libfoo"], {"wine-core"})
        self.assertEqual(reverse["wine-core"], {"wine"})
        hard, combined, relations, _complete = MODULE.rpm_dependency_graph()
        self.assertNotIn("dosbox-staging", hard)
        self.assertEqual(combined["dosbox-staging"], {"wine"})
        self.assertEqual(
            relations[("dosbox-staging", "wine")], {"recommends"})

    @patch.object(MODULE, "capture")
    def test_rpm_reverse_weak_dependencies_point_to_the_install_cause(
            self, capture):
        capture.return_value = (
            "P\tbase-app\nS\tbase-app\n"
            "P\taddon\nU\tbase-app\nS\taddon\n"
        )
        _hard, combined, relations, complete = MODULE.rpm_dependency_graph()
        self.assertTrue(complete)
        self.assertEqual(combined["addon"], {"base-app"})
        self.assertEqual(
            relations[("addon", "base-app")], {"is supplemented by"})
        paths, _complete = MODULE.relationship_root_paths(
            "addon", combined, relations, {"base-app"})
        self.assertEqual(
            paths,
            [(["base-app", "addon"], ["is supplemented by"])],
        )

    def test_root_cause_paths_stop_at_explicit_applications(self):
        reverse = {
            "libfoo": {"wine-core", "tex-engine"},
            "wine-core": {"wine"},
            "tex-engine": {"texlive"},
        }
        paths, complete = MODULE.root_paths(
            "libfoo", reverse, {"wine", "texlive"})
        self.assertTrue(complete)
        self.assertEqual(
            paths,
            [["texlive", "tex-engine", "libfoo"],
             ["wine", "wine-core", "libfoo"]],
        )

    def test_relationship_paths_retain_weak_edge_labels(self):
        reverse = {
            "dosbox-staging": {"wine"},
        }
        relations = {
            ("dosbox-staging", "wine"): {"recommends"},
        }
        paths, complete = MODULE.relationship_root_paths(
            "dosbox-staging", reverse, relations, {"wine"})
        self.assertTrue(complete)
        self.assertEqual(
            paths,
            [(["wine", "dosbox-staging"], ["recommends"])],
        )

    def test_dependency_output_does_not_repeat_a_direct_weak_path(self):
        item = MODULE.Match(
            "DNF", "dosbox-staging", "dosbox-staging",
            role="weak dependency",
        )
        report = MODULE.DependencyReport(
            item, [], [], True,
            weak_relations=[("wine", "recommends")],
            relationship_paths=[
                (["wine", "dosbox-staging"], ["recommends"]),
            ],
        )
        output = io.StringIO()
        with redirect_stdout(output):
            MODULE.show_dependency_reports([report])
        rendered = output.getvalue()
        self.assertEqual(
            rendered.count("wine --recommends--> dosbox-staging"), 1)
        self.assertNotIn("Other optional relationships", rendered)

    @patch.object(MODULE, "capture")
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_dnf_history_reports_original_install_command(
            self, _which, capture):
        capture.side_effect = [
            MODULE.json.dumps([{
                "id": 193,
                "command_line": "dnf install wine",
            }]),
            MODULE.json.dumps([{
                "id": 193,
                "description": "dnf install wine",
                "packages": [{
                    "nevra": "dosbox-staging-0:0.82.2-3.fc42.x86_64",
                    "action": "Install",
                    "reason": "Weak Dependency",
                }],
            }]),
        ]
        self.assertEqual(
            MODULE.dnf_history_reason("dosbox-staging"),
            "DNF transaction 193: dnf install wine "
            "(recorded reason: Weak Dependency)",
        )

    @patch.object(MODULE, "capture")
    def test_apt_graph_understands_alternative_dependencies(self, capture):
        capture.return_value = (
            "P\tii \tlibfoo:amd64\t\t\tvirtual-foo\n"
            "P\tii \twine\tvirtual-foo | other (>= 2), libc6\t\t\n"
            "P\trc \tremoved-app\tlibfoo\t\t\n"
        )
        reverse, complete = MODULE.apt_reverse_graph()
        self.assertTrue(complete)
        self.assertEqual(reverse["libfoo"], {"wine"})

    def test_dnf_preview_separates_unused_dependencies(self):
        output = (
            "Removing:\n"
            " target x86_64 1 repo 1 MiB\n"
            "Removing dependent packages:\n"
            " wine x86_64 1 repo 2 MiB\n"
            "Removing unused dependencies:\n"
            " helper x86_64 1 repo 3 MiB\n\n"
            "Transaction Summary:\n"
        )
        self.assertEqual(
            MODULE.parse_dnf_preview(output),
            (["target", "wine", "helper"], ["helper"]),
        )

    @patch.object(MODULE, "capture_any")
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_dnf_native_preview_is_read_only(self, _which, capture_any):
        capture_any.return_value = (
            1,
            "Removing:\n target x86_64 1 repo 1 MiB\n\n"
            "Transaction Summary:\nOperation aborted by the user.\n",
        )
        item = MODULE.Match("DNF", "target", "target")
        planned, _orphans, available, _notes = (
            MODULE.native_removal_preview([item]))
        self.assertTrue(available)
        self.assertEqual(planned, ["target"])
        capture_any.assert_called_once_with(
            ["dnf5", "--assumeno", "remove", "target"])

    @patch.object(MODULE, "capture_any", return_value=(0, ""))
    def test_plain_rpm_fallback_uses_test_transaction(self, capture_any):
        item = MODULE.Match("RPM", "target", "target")
        planned, _orphans, available, _notes = (
            MODULE.native_removal_preview([item]))
        self.assertTrue(available)
        self.assertEqual(planned, ["target"])
        capture_any.assert_called_once_with(
            ["rpm", "-e", "--test", "target"])
        with patch.object(
                MODULE, "privileged", side_effect=lambda command: command):
            self.assertEqual(
                MODULE.uninstall_command(item, False),
                ["rpm", "-e", "target"],
            )

    def test_core_packages_are_always_marked_protected(self):
        self.assertEqual(
            MODULE.protection_reason("APT", "systemd"),
            "core system package",
        )

    @patch.object(MODULE, "protection_reason", return_value="Essential package")
    @patch.object(MODULE, "native_removal_preview")
    @patch.object(MODULE, "dependency_report")
    def test_protected_package_makes_plan_high_impact(
            self, dependency_report, native_preview, _protection):
        item = MODULE.Match("APT", "core", "core", role="explicit")
        dependency_report.return_value = MODULE.DependencyReport(
            item, [], [], True)
        native_preview.return_value = (["core"], [], True, [])
        plan = MODULE.build_removal_plan([item])
        self.assertEqual(plan.level, "HIGH")
        self.assertEqual(
            plan.protected_items, ["core (Essential package)"])

    @patch.object(MODULE, "protection_reason", return_value="")
    @patch.object(MODULE, "native_removal_preview")
    @patch.object(MODULE, "dependency_report")
    def test_dependent_application_makes_plan_high_impact(
            self, dependency_report, native_preview, _protection):
        item = MODULE.Match(
            "DNF", "libfoo", "libfoo", role="dependency")
        dependency_report.return_value = MODULE.DependencyReport(
            item, ["wine-core"], [["wine", "wine-core", "libfoo"]])
        native_preview.return_value = (
            ["libfoo", "wine-core", "wine"], [], True, [])
        plan = MODULE.build_removal_plan([item])
        self.assertEqual(plan.level, "HIGH")
        self.assertEqual(plan.additional_removals, ["wine-core", "wine"])

    @patch.object(MODULE.subprocess, "run")
    @patch.object(MODULE, "find_user_data", return_value=[])
    @patch.object(MODULE, "build_removal_plan")
    @patch.object(MODULE, "filter_dependency_matches")
    @patch.object(MODULE, "annotate_roles")
    @patch.object(MODULE, "find_matches")
    def test_high_impact_requires_typed_confirmation_not_yes(
            self, find_matches, annotate, filter_matches, build_plan,
            _find_data, run):
        item = MODULE.Match("Cargo", "tool", "tool", role="explicit")
        find_matches.return_value = [item]
        annotate.return_value = [item]
        filter_matches.return_value = ([item], 0)
        build_plan.return_value = MODULE.RemovalPlan(
            [item], [MODULE.DependencyReport(item, ["other"], [])],
            ["tool", "other"], ["other"], [], "HIGH", True, [], [])
        with patch("builtins.input", return_value="y"):
            self.assertEqual(MODULE.run_uninstall("tool"), 0)
        run.assert_not_called()

    @patch.object(MODULE.subprocess, "run")
    @patch.object(MODULE, "build_removal_plan")
    @patch.object(MODULE, "filter_dependency_matches")
    @patch.object(MODULE, "annotate_roles")
    @patch.object(MODULE, "find_matches")
    def test_plan_mode_never_executes_removal(
            self, find_matches, annotate, filter_matches, build_plan, run):
        item = MODULE.Match("Cargo", "tool", "tool", role="explicit")
        find_matches.return_value = [item]
        annotate.return_value = [item]
        filter_matches.return_value = ([item], 0)
        build_plan.return_value = MODULE.RemovalPlan(
            [item], [MODULE.DependencyReport(item, [], [])],
            ["tool"], [], [], "LOW", True, [], [])
        with patch("builtins.input", side_effect=AssertionError(
                "a unique read-only result must not prompt")):
            self.assertEqual(
                MODULE.run_uninstall("tool", plan_only=True), 0)
        run.assert_not_called()

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
        capture.side_effect = ["", "httpie 3.2.4\n", "", ""]
        result = MODULE.detect_pipx("httpie")
        self.assertEqual(
            (result[0].kind, result[0].ident, result[0].version),
            ("Pipx", "httpie", "3.2.4"),
        )

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/pipx")
    @patch.object(MODULE, "capture")
    def test_pipx_prefers_machine_readable_inventory(self, capture, _which):
        capture.side_effect = [
            MODULE.json.dumps({
                "pipx_spec_version": "0.1",
                "venvs": {
                    "httpie": {
                        "metadata": {
                            "main_package": {
                                "package": "httpie",
                                "package_version": "3.2.4",
                            },
                        },
                    },
                },
            }),
            "{}",
        ]
        result = MODULE.detect_pipx("httpie")
        self.assertEqual(
            (result[0].ident, result[0].version),
            ("httpie", "3.2.4"),
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

    def test_nix_profile_json_parser_handles_current_schema(self):
        text = MODULE.json.dumps({
            "version": 3,
            "elements": {
                "hello": {
                    "storePaths": ["/nix/store/abc-hello"],
                },
            },
        })
        self.assertEqual(
            MODULE.parse_nix_profile_json(text),
            [("hello", ["/nix/store/abc-hello"])],
        )

    def test_commands_are_exact_and_do_not_use_a_shell(self):
        item = MODULE.Match("Flatpak", "org.freecad.FreeCAD", "FreeCAD",
                            "1.0", "user")
        self.assertEqual(
            MODULE.uninstall_command(item, True),
            ["flatpak", "uninstall", "-y", "--user", "--delete-data",
             "org.freecad.FreeCAD"],
        )

    def test_homebrew_cask_cleanup_uses_zap(self):
        item = MODULE.Match(
            "Homebrew Cask", "firefox", "firefox", scope="user")
        self.assertEqual(
            MODULE.uninstall_command(item, True),
            ["brew", "uninstall", "--cask", "--zap", "firefox"],
        )

    @patch.object(MODULE.os, "geteuid", return_value=0)
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_same_backend_selections_run_as_one_transaction(
            self, _which, _euid):
        selected = [
            MODULE.Match("DNF", "first", "first"),
            MODULE.Match("DNF", "second", "second"),
        ]
        self.assertEqual(
            MODULE.build_uninstall_batches(selected, False),
            [(
                selected,
                ["dnf5", "remove", "first", "second"],
            )],
        )

    def test_snap_cleanup_means_purge_not_manual_home_deletion(self):
        item = MODULE.Match("Snap", "example", "example", scope="system")
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            snap_data = home / "snap" / "example"
            snap_data.mkdir(parents=True)
            with patch.object(MODULE.Path, "home", return_value=home):
                self.assertNotIn(snap_data, MODULE.find_user_data([item]))
        with patch.object(
                MODULE, "privileged", side_effect=lambda command: command):
            self.assertEqual(
                MODULE.uninstall_command(item, True),
                ["snap", "remove", "--purge", "example"],
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

    def test_gearlever_desktop_entry_is_manager_owned_not_optional_data(self):
        with tempfile.TemporaryDirectory() as directory:
            data_home = Path(directory)
            applications = data_home / "applications"
            applications.mkdir()
            desktop = applications / "arduino_ide.desktop"
            appimage = Path(directory) / "arduino_ide.appimage"
            appimage.touch()
            desktop.write_text(
                f"[Desktop Entry]\nTryExec={appimage}\nExec={appimage} %U\n",
                encoding="utf-8",
            )
            selected = [
                MODULE.Match(
                    "Gear Lever", str(appimage), "Arduino IDE", path=appimage)
            ]
            with patch.dict(os.environ, {"XDG_DATA_HOME": str(data_home)}):
                self.assertNotIn(desktop, MODULE.find_user_data(selected))

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
        with patch("builtins.input", side_effect=["y", "a", "y"]):
            self.assertEqual(MODULE.run_uninstall("Example"), 1)
        remove_paths.assert_not_called()
        self.assertIn("--delete-data", run.call_args.args[0])

    def test_user_can_choose_individual_cleanup_paths(self):
        paths = [Path("/tmp/first"), Path("/tmp/second")]
        with patch("builtins.input", return_value="2"):
            self.assertEqual(
                MODULE.choose_cleanup_paths(paths),
                [paths[1]],
            )

    def test_manager_cleanup_choices_are_independent(self):
        selected = [
            MODULE.Match(
                "Flatpak", "org.example.App", "Example", scope="user"),
            MODULE.Match("APT", "example", "example"),
        ]
        with patch("builtins.input", side_effect=["y", "n"]):
            cleanup_kinds, paths = MODULE.ask_cleanup(selected, [])
        self.assertEqual(cleanup_kinds, {"Flatpak"})
        self.assertEqual(paths, [])
        with patch.object(
                MODULE, "privileged", side_effect=lambda command: command):
            batches = MODULE.build_uninstall_batches(
                selected, cleanup_kinds)
        self.assertIn("--delete-data", batches[0][1])
        self.assertEqual(batches[1][1][:2], ["apt-get", "remove"])

    @patch.object(MODULE.subprocess, "run")
    @patch.object(MODULE, "find_user_data", return_value=[])
    @patch.object(MODULE, "find_matches")
    def test_cancel_at_final_confirmation_runs_nothing(
            self, find_matches, _find_user_data, run):
        find_matches.return_value = [
            MODULE.Match("Cargo", "tool", "tool", scope="user")
        ]
        with patch("builtins.input", return_value=""):
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

    def test_read_only_mode_auto_selects_one_result(self):
        item = MODULE.Match("DNF", "dosbox-staging", "dosbox-staging")
        with patch("builtins.input", side_effect=AssertionError(
                "read-only unique result must not prompt")):
            self.assertEqual(
                MODULE.choose([item], auto_select=True),
                [item],
            )

    @patch.object(MODULE, "self_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall"])
    def test_uninstall_uninstall_is_self_uninstall(self, self_uninstall):
        self.assertEqual(MODULE.main(), 0)
        self_uninstall.assert_called_once_with()

    @patch.object(MODULE, "self_uninstall", return_value=0)
    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(
        MODULE.sys, "argv", ["uninstall", "uninstall", "--why"])
    def test_uninstall_why_does_not_trigger_self_removal(
            self, run_uninstall, self_uninstall):
        self.assertEqual(MODULE.main(), 0)
        self_uninstall.assert_not_called()
        run_uninstall.assert_called_once_with(
            "uninstall", show_dependencies=False,
            plan_only=False, why_only=True,
        )

    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall-helper"])
    def test_longer_name_remains_a_normal_search(self, run_uninstall):
        self.assertEqual(MODULE.main(), 0)
        run_uninstall.assert_called_once_with(
            "uninstall-helper", show_dependencies=False,
            plan_only=False, why_only=False,
        )

    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "--why"])
    def test_no_argument_prompts_for_an_app(self, run_uninstall):
        with patch("builtins.input", return_value="DOSbox"):
            self.assertEqual(MODULE.main(), 0)
        run_uninstall.assert_called_once_with(
            "DOSbox", show_dependencies=False,
            plan_only=False, why_only=True,
        )

    @patch.object(MODULE.os, "geteuid", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "freecad"])
    def test_running_whole_program_through_sudo_is_refused(self, _euid):
        with patch.dict(os.environ, {"SUDO_USER": "test"}, clear=False):
            self.assertEqual(MODULE.main(), 2)


if __name__ == "__main__":
    unittest.main()
