import hashlib
import importlib.machinery
import importlib.util
import io
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from threading import Barrier, Event, Lock
from unittest.mock import call, patch

SCRIPT = Path(__file__).parents[1] / "uninstall"
INSTALLER = Path(__file__).parents[1] / "install.sh"
CHECKSUM = Path(__file__).parents[1] / "uninstall.sha256"
LOADER = importlib.machinery.SourceFileLoader("uninstall_cli", str(SCRIPT))
SPEC = importlib.util.spec_from_loader("uninstall_cli", LOADER)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class UninstallTests(unittest.TestCase):
    def setUp(self):
        MODULE.norm.cache_clear()
        MODULE.rpm_inventory.cache_clear()
        MODULE.rpm_dependency_inventory.cache_clear()
        MODULE.rpm_inventory_by_name.cache_clear()
        MODULE.apt_inventory.cache_clear()
        MODULE.apt_inventory_by_name.cache_clear()
        MODULE.pacman_inventory.cache_clear()
        MODULE.pacman_inventory_by_name.cache_clear()
        MODULE.apk_inventory.cache_clear()
        MODULE.apk_inventory_by_name.cache_clear()
        MODULE.apk_world.cache_clear()
        MODULE.apk_provider_map.cache_clear()
        MODULE.apk_explicit_causes.cache_clear()
        MODULE.apk_reverse_graph.cache_clear()
        MODULE.opkg_inventory.cache_clear()
        MODULE.opkg_inventory_by_name.cache_clear()
        MODULE.opkg_provider_map.cache_clear()
        MODULE.opkg_reverse_graph.cache_clear()
        MODULE.xbps_inventory.cache_clear()
        MODULE.xbps_inventory_by_name.cache_clear()
        MODULE.xbps_manual_packages.cache_clear()
        MODULE.xbps_reverse_dependencies.cache_clear()
        MODULE.portage_inventory.cache_clear()
        MODULE.portage_inventory_by_name.cache_clear()
        MODULE.portage_world.cache_clear()
        MODULE.slackware_inventory.cache_clear()
        MODULE.slackware_inventory_by_name.cache_clear()
        MODULE.eopkg_inventory.cache_clear()
        MODULE.eopkg_automatic_packages.cache_clear()
        MODULE.swupd_inventory.cache_clear()
        MODULE.swupd_reverse_dependencies.cache_clear()
        MODULE.flatpak_inventory.cache_clear()
        MODULE.desktop_inventory.cache_clear()
        MODULE.appstream_inventory.cache_clear()
        MODULE.snap_inventory.cache_clear()
        MODULE.pipx_inventory.cache_clear()
        MODULE.uv_tool_inventory.cache_clear()
        MODULE.conda_environments.cache_clear()
        MODULE.npm_global_prefix.cache_clear()
        MODULE.npm_global_root.cache_clear()
        MODULE.npm_inventory.cache_clear()
        MODULE.homebrew_installed_metadata.cache_clear()
        MODULE.cargo_install_records.cache_clear()
        MODULE.nix_env_inventory.cache_clear()
        MODULE.guix_inventory.cache_clear()
        MODULE.rpm_ostree_layered_packages.cache_clear()
        MODULE.zypper_userinstalled.cache_clear()
        MODULE.dnf_install_reasons.cache_clear()
        MODULE.dnf_history_transaction.cache_clear()
        MODULE.dnf_install_record.cache_clear()
        MODULE.dnf_history_reason.cache_clear()
        MODULE.dnf_installed_group_inventory.cache_clear()
        MODULE.dnf_group_memberships.cache_clear()
        MODULE.dnf_installed_environment_inventory.cache_clear()
        MODULE.dnf_environment_details.cache_clear()
        MODULE.apt_history_event.cache_clear()
        MODULE.apt_history_index.cache_clear()
        MODULE.pacman_history_event.cache_clear()
        MODULE.pacman_history_index.cache_clear()
        MODULE.zypper_history_event.cache_clear()
        MODULE.zypper_history_index.cache_clear()
        MODULE.legacy_rpm_history_event.cache_clear()
        MODULE.flatpak_history_entries.cache_clear()
        MODULE.flatpak_install_evidence.cache_clear()
        MODULE.snap_install_evidence.cache_clear()
        MODULE.snap_changes.cache_clear()
        MODULE.homebrew_install_evidence.cache_clear()
        MODULE.nix_profile_metadata.cache_clear()
        MODULE.cargo_install_source.cache_clear()
        MODULE.pipx_install_source.cache_clear()
        MODULE.npm_install_source.cache_clear()
        MODULE.rpm_install_metadata.cache_clear()
        MODULE.rpm_dependency_graph.cache_clear()
        MODULE.rpm_reverse_graph.cache_clear()
        MODULE.apt_reverse_graph.cache_clear()
        MODULE.pacman_reverse_graph.cache_clear()
        MODULE.explicit_names_for_kind.cache_clear()
        MODULE.apt_held_packages.cache_clear()
        MODULE.dnf_protected_patterns.cache_clear()
        MODULE.path_disk_size.cache_clear()
        MODULE.package_installed_size.cache_clear()
        MODULE.transactional_zypper_system.cache_clear()
        MODULE.os_release.cache_clear()
        MODULE.immutable_host.cache_clear()
        MODULE.runtime_protected_packages.cache_clear()
        MODULE.nix_profile_metadata.cache_clear()
        MODULE._DIAGNOSTICS.clear()
        MODULE._CLEANUP_SNAPSHOTS.clear()
        MODULE._DNF_HISTORY_DETAILS.clear()
        MODULE._DNF_INSTALL_RECORDS.clear()

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

    def test_published_source_checksum_matches_release_artifact(self):
        expected = CHECKSUM.read_text(encoding="utf-8").split()[0]
        actual = hashlib.sha256(SCRIPT.read_bytes()).hexdigest()
        self.assertEqual(actual, expected)

    def test_release_versions_do_not_drift(self):
        installer = INSTALLER.read_text(encoding="utf-8")
        self.assertIn(f"RELEASE_VERSION={MODULE.VERSION}", installer)
        self.assertIn(f"uninstall {MODULE.VERSION}", (
            Path(__file__).parents[1] / "docs/uninstall.1"
        ).read_text(encoding="utf-8"))

    def test_matching_ignores_case_and_punctuation(self):
        self.assertTrue(MODULE.relevant("FreeCAD", "org.freecad.FreeCAD"))
        self.assertTrue(MODULE.relevant("visual studio", "visual-studio-code"))
        self.assertFalse(MODULE.relevant("freecad", "Firefox"))
        self.assertTrue(MODULE.relevant("éditeur", "Éditeur"))
        self.assertFalse(MODULE.relevant("rg", "org.mozilla.firefox"))
        self.assertTrue(MODULE.relevant("rg", "rg"))

    def test_sizes_use_compact_binary_units(self):
        self.assertEqual(MODULE.format_size(0), "0 B")
        self.assertEqual(MODULE.format_size(28 * 1024), "28 KiB")
        self.assertEqual(
            MODULE.format_size(1533637632),
            "1.4 GiB",
        )

    @patch.object(MODULE, "capture", return_value="1533637632\n")
    def test_flatpak_installed_size_is_machine_readable(self, capture):
        self.assertEqual(
            MODULE.package_installed_size(
                "Flatpak", "org.freecad.FreeCAD", "system"),
            1533637632,
        )
        capture.assert_called_once_with([
            "flatpak", "info", "--system", "--show-size",
            "org.freecad.FreeCAD",
        ])

    @patch.object(MODULE, "rpm_inventory_by_name")
    def test_rpm_size_sums_multiple_installed_architectures(self, inventory):
        inventory.return_value = {
            "example": (
                MODULE.RpmPackageRecord(
                    "example", "1", "", 1000, "", ""),
                MODULE.RpmPackageRecord(
                    "example", "1", "", 2000, "", ""),
            ),
        }
        self.assertEqual(
            MODULE.package_installed_size(
                "DNF", "example", "system"),
            3000,
        )
        with patch.object(
                MODULE.shutil, "which", return_value="/usr/bin/rpm"), \
                patch.object(MODULE, "rpm_manager", return_value="DNF"):
            [detected] = MODULE.detect_rpm("example")
        self.assertEqual(detected.size_bytes, 3000)

    @patch.object(MODULE, "capture", return_value="2048")
    def test_apt_installed_size_converts_kibibytes(self, _capture):
        self.assertEqual(
            MODULE.package_installed_size(
                "APT", "example", "system"),
            2 * 1024 * 1024,
        )

    @patch.object(
        MODULE, "capture",
        return_value="Name : example\nInstalled Size : 2.3 MiB\n",
    )
    def test_pacman_installed_size_is_parsed(self, _capture):
        self.assertEqual(
            MODULE.package_installed_size(
                "Pacman", "example", "system"),
            int(2.3 * 1024 * 1024),
        )

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/tool")
    @patch.object(MODULE, "capture")
    def test_rpm_search_inventory_defers_large_dependency_metadata(
            self, capture, _which):
        lightweight = (
            "P\texample\t1.2-3\tExample application\t4096\t"
            "Tue Aug  4 10:00:00 2026\tExample Vendor\n"
            "P\tlibexample\t1.0-1\tExample library\t2048\t"
            "Mon Aug  3 10:00:00 2026\tExample Vendor\n"
        )
        relationships = (
            "P\texample\n"
            "R\tlibexample.so.1()(64bit)\n"
            "S\texample\n"
            "P\tlibexample\n"
            "S\tlibexample.so.1()(64bit)\n"
        )
        capture.side_effect = [lightweight, relationships]

        found = MODULE.detect_rpm("example")
        reverse, complete = MODULE.rpm_reverse_graph()
        size = MODULE.package_installed_size(
            "DNF", "example", "system")
        metadata = MODULE.rpm_install_metadata("example")

        self.assertEqual([item.ident for item in found], ["example", "libexample"])
        self.assertTrue(complete)
        self.assertEqual(reverse["libexample"], {"example"})
        self.assertEqual(size, 4096)
        self.assertEqual(metadata, (
            "Tue Aug  4 10:00:00 2026", "Example Vendor"))
        self.assertEqual(capture.call_count, 2)

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dpkg-query")
    @patch.object(MODULE, "capture")
    def test_apt_inventory_is_shared_by_search_graph_and_size(
            self, capture, _which):
        capture.return_value = (
            "ii \tapp\t1.0\tExample app\t4\tlibfoo\t\t\n"
            "ii \tlibfoo\t1.0\tExample library\t2\t\t\tvirtual-foo\n"
        )
        [item] = MODULE.detect_dpkg("Example app")
        reverse, complete = MODULE.apt_reverse_graph()
        size = MODULE.package_installed_size("APT", "app", "system")

        self.assertTrue(complete)
        self.assertEqual(reverse["libfoo"], {"app"})
        self.assertEqual(size, 4096)
        self.assertEqual(item.size_bytes, 4096)
        capture.assert_called_once()

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/pacman")
    @patch.object(MODULE, "capture")
    def test_pacman_inventory_is_shared_by_search_role_graph_and_size(
            self, capture, _which):
        capture.return_value = (
            "Name : app\nVersion : 1.0\nInstalled Size : 4 KiB\n"
            "Install Reason : Explicitly installed\nRequired By : None\n\n"
            "Name : libfoo\nVersion : 1.0\nInstalled Size : 2 KiB\n"
            "Install Reason : Installed as a dependency\nRequired By : app\n"
        )
        [item] = MODULE.detect_pacman("app")
        [annotated] = MODULE.annotate_roles([item])
        reverse, complete = MODULE.pacman_reverse_graph()
        size = MODULE.package_installed_size("Pacman", "app", "system")

        self.assertEqual(annotated.role, "explicit")
        self.assertTrue(complete)
        self.assertEqual(reverse["libfoo"], {"app"})
        self.assertEqual(size, 4096)
        capture.assert_called_once()

    @patch.object(MODULE.shutil, "which", return_value="/sbin/apk")
    def test_apk_inventory_world_roles_and_dependency_roots(self, _which):
        records = MODULE.parse_apk_installed(
            "P:nano\nV:8.0-r0\nT:Small editor\nI:4096\nD:musl\n"
            "p:cmd:nano\no:nano\n\n"
            "P:musl\nV:1.2.5-r1\nT:C library\nI:2048\n"
            "p:so:libc.musl-x86_64.so.1\n"
        )
        with patch.object(MODULE, "apk_inventory", return_value=records), \
                patch.object(MODULE, "apk_world", return_value=("nano",)):
            [app] = MODULE.annotate_roles(MODULE.detect_apk("Small editor"))
            [library] = MODULE.annotate_roles(MODULE.detect_apk("musl"))
            report = MODULE.dependency_report(library)
            reason = MODULE.install_reason(app)

        self.assertEqual((app.role, app.size_bytes), ("explicit", 4096))
        self.assertEqual(library.role, "dependency")
        self.assertEqual(report.direct_dependents, ["nano"])
        self.assertEqual(report.root_paths, [["nano", "musl"]])
        self.assertEqual(
            reason,
            "listed explicitly in /etc/apk/world; "
            "original APK command unavailable",
        )

    @patch.object(MODULE, "capture_any")
    def test_apk_preview_includes_automatically_removed_dependencies(
            self, capture_any):
        capture_any.return_value = (
            0,
            ("(1/2) Purging nano (8.0-r0)\n"
            "(2/2) Purging oniguruma (6.9-r0)\n"),
        )
        selected = [MODULE.Match("APK", "nano", "nano", scope="system")]
        planned, orphans, available, _notes = (
            MODULE.native_removal_preview(selected))
        self.assertEqual(planned, ["nano", "oniguruma"])
        self.assertEqual(orphans, ["oniguruma"])
        self.assertTrue(available)
        capture_any.assert_called_once_with([
            "apk", "del", "--simulate", "nano",
        ])

    def test_apk_world_repository_tag_still_marks_provider_explicit(self):
        records = (MODULE.PackageRecord("nano", provides=("editor",)),)
        with patch.object(MODULE, "apk_inventory", return_value=records), \
                patch.object(MODULE, "apk_world", return_value=("editor@edge>=8",)):
            self.assertEqual(
                MODULE.apk_explicit_causes(), {"nano": "editor@edge>=8"})

    @patch.object(MODULE.shutil, "which", return_value="/bin/opkg")
    def test_opkg_status_roles_sizes_and_dependency_roots(self, _which):
        records = MODULE.parse_opkg_status(
            "Package: nano\nVersion: 8.0-1\n"
            "Status: install user installed\nInstalled-Size: 4096\n"
            "Depends: libfoo\nDescription: Small editor\n\n"
            "Package: libfoo\nVersion: 2.0-1\n"
            "Status: install ok installed\nAuto-Installed: yes\n"
            "Installed-Size: 2048\nDescription: Runtime library\n"
        )
        with patch.object(MODULE, "opkg_inventory", return_value=records):
            [app] = MODULE.annotate_roles(MODULE.detect_opkg("Small editor"))
            [library] = MODULE.annotate_roles(MODULE.detect_opkg("libfoo"))
            report = MODULE.dependency_report(library)

        self.assertEqual((app.role, app.size_bytes), ("explicit", 4096))
        self.assertEqual(library.role, "dependency")
        self.assertEqual(report.direct_dependents, ["nano"])
        self.assertEqual(report.root_paths, [["nano", "libfoo"]])

    @patch.object(MODULE, "capture_any", return_value=(
        0, "Removing package nano from root...\n"))
    def test_opkg_uses_a_no_action_removal_preview(self, capture_any):
        selected = [MODULE.Match("OPKG", "nano", "nano", scope="system")]
        planned, _orphans, available, _notes = (
            MODULE.native_removal_preview(selected))
        self.assertEqual(planned, ["nano"])
        self.assertTrue(available)
        capture_any.assert_called_once_with([
            "opkg", "--noaction", "remove", "nano",
        ])

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/xbps-query")
    @patch.object(MODULE, "xbps_reverse_dependencies")
    @patch.object(MODULE, "xbps_manual_packages")
    @patch.object(MODULE, "xbps_inventory")
    def test_xbps_manual_roles_and_reverse_dependency_roots(
            self, inventory, manual, reverse, _which):
        inventory.return_value = (
            MODULE.PackageRecord("app", "1.0_1", "Application"),
            MODULE.PackageRecord("libfoo", "2.0_1", "Library"),
        )
        manual.return_value = (True, {"app"})
        reverse.side_effect = lambda target: (
            (True, {"app"}) if target == "libfoo" else (True, set()))

        [app] = MODULE.annotate_roles(MODULE.detect_xbps("Application"))
        [library] = MODULE.annotate_roles(MODULE.detect_xbps("libfoo"))
        report = MODULE.dependency_report(library)

        self.assertEqual(app.role, "explicit")
        self.assertEqual(library.role, "dependency")
        self.assertEqual(report.direct_dependents, ["app"])
        self.assertEqual(report.root_paths, [["app", "libfoo"]])

    @patch.object(MODULE, "capture_any", return_value=(
        0,
        ("nano-8.0_1 remove x86_64 repo 4096 0\n"
        "oniguruma-6.9_1 remove x86_64 repo 2048 0\n"),
    ))
    def test_xbps_recursive_dry_run_is_parsed(self, capture_any):
        selected = [MODULE.Match("XBPS", "nano", "nano", scope="system")]
        planned, orphans, available, _notes = (
            MODULE.native_removal_preview(selected))
        self.assertEqual(planned, ["nano", "oniguruma"])
        self.assertEqual(orphans, ["oniguruma"])
        self.assertTrue(available)
        capture_any.assert_called_once_with([
            "xbps-remove", "--dry-run", "--recursive", "nano",
        ])

    def test_portage_vdb_world_and_exact_depclean_command(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "app-editors" / "nano-8.0"
            package.mkdir(parents=True)
            (package / "PN").write_text("nano\n", encoding="utf-8")
            (package / "PVR").write_text("8.0\n", encoding="utf-8")
            (package / "DESCRIPTION").write_text(
                "Small editor\n", encoding="utf-8")
            (package / "SIZE").write_text("4096\n", encoding="utf-8")
            (package / "repository").write_text(
                "gentoo\n", encoding="utf-8")
            (package / "CONTENTS").write_text(
                "obj /usr/bin/nano hash 1\n", encoding="utf-8")
            records = MODULE.parse_portage_vdb(Path(directory))

        self.assertEqual(
            (records[0].ident, records[0].version, records[0].size_bytes),
            ("app-editors/nano", "8.0", 4096),
        )
        with patch.object(MODULE, "portage_inventory", return_value=records), \
                patch.object(
                    MODULE, "portage_world",
                    return_value={"app-editors/nano"}), \
                patch.object(MODULE.shutil, "which", return_value="/usr/bin/emerge"):
            [item] = MODULE.annotate_roles(MODULE.detect_portage("nano"))
        self.assertEqual(item.role, "explicit")
        self.assertIn("Portage @world", MODULE.install_reason(item))
        with patch.object(MODULE.os, "geteuid", return_value=0):
            self.assertEqual(
                MODULE.uninstall_command(item, False),
                [
                    "emerge", "--ask=n", "--verbose", "--depclean",
                    "=app-editors/nano-8.0",
                ],
            )

    def test_slackware_log_preserves_name_size_description_and_files(self):
        record = MODULE.parse_slackware_package_log(
            "PACKAGE NAME: nano-8.0-x86_64-1\n"
            "UNCOMPRESSED PACKAGE SIZE: 4M\n"
            "PACKAGE DESCRIPTION:\n"
            "nano: Small editor\n"
            "FILE LIST:\n"
            "./\nusr/bin/nano\nusr/share/nano/\n",
            "fallback",
        )
        self.assertEqual((record.ident, record.version), ("nano", "8.0"))
        self.assertEqual(record.summary, "Small editor")
        self.assertEqual(record.size_bytes, 4 * 1024 * 1024)
        self.assertEqual(record.files, ("/usr/bin/nano",))

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/eopkg")
    @patch.object(MODULE, "capture", return_value=(
        "nano - Small editor\nlibfoo - Runtime library\n"))
    def test_eopkg_installed_inventory_is_searchable(
            self, capture, _which):
        [item] = MODULE.detect_eopkg("Small editor")
        self.assertEqual((item.kind, item.ident), ("Eopkg", "nano"))
        capture.assert_called_once_with([
            "eopkg", "--no-color", "list-installed",
        ])

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/eopkg")
    @patch.object(MODULE, "capture_any", return_value=(
        0, "libfoo - nano\norphan - Orphaned package\n"))
    def test_eopkg_uses_retained_automatic_parent_for_role_and_reason(
            self, capture_any, _which):
        item = MODULE.Match("Eopkg", "libfoo", "libfoo")
        [annotated] = MODULE.annotate_roles([item])
        self.assertEqual(annotated.role, "dependency")
        self.assertEqual(
            MODULE.install_reason(annotated),
            "installed automatically for nano; original Eopkg command unavailable",
        )
        capture_any.assert_called_once_with([
            "eopkg", "--no-color", "list-installed", "--automatic",
        ], timeout=30)

    def test_new_system_backends_use_native_non_force_removal_commands(self):
        cases = (
            (MODULE.Match("APK", "nano", "nano", scope="system"),
             ["apk", "del", "nano"]),
            (MODULE.Match("OPKG", "nano", "nano", scope="system"),
             ["opkg", "remove", "nano"]),
            (MODULE.Match("XBPS", "nano", "nano", scope="system"),
             ["xbps-remove", "--recursive", "--yes", "nano"]),
            (MODULE.Match(
                "Portage", "app-editors/nano", "nano", "8.0", "system"),
             ["emerge", "--ask=n", "--verbose", "--depclean",
              "=app-editors/nano-8.0"]),
            (MODULE.Match("Slackware", "nano", "nano", scope="system"),
             ["removepkg", "nano"]),
            (MODULE.Match("Eopkg", "nano", "nano", scope="system"),
             ["eopkg", "remove", "--yes-all", "nano"]),
        )
        with patch.object(MODULE.os, "geteuid", return_value=0):
            for item, expected in cases:
                with self.subTest(kind=item.kind):
                    self.assertEqual(
                        MODULE.uninstall_command(item, False), expected)

    def test_owner_output_matches_apk_xbps_and_eopkg_packages_exactly(self):
        record = MODULE.PackageRecord("nano", "8.0-r0", size_bytes=4096)
        visible = Path("/usr/bin/nano")
        for kind, output in (
                ("APK", "/usr/bin/nano is owned by nano-8.0-r0"),
                ("XBPS", "nano-8.0-r0: /usr/bin/nano (regular file)"),
                ("Eopkg", "nano package contains /usr/bin/nano")):
            with self.subTest(kind=kind):
                [item] = MODULE.package_owner_from_output(
                    output, (record,), kind, visible)
                self.assertEqual((item.ident, item.provides),
                                 ("nano", "/usr/bin/nano"))

    @patch.object(MODULE, "capture_any", return_value=(0, "safe preview"))
    def test_portage_and_slackware_unparsed_previews_remain_unknown(
            self, capture_any):
        portage = MODULE.Match(
            "Portage", "app-editors/nano", "nano", "8.0", "system")
        slackware = MODULE.Match(
            "Slackware", "nano", "nano", "8.0", "system")
        for item in (portage, slackware):
            with self.subTest(kind=item.kind):
                planned, _orphans, available, notes = (
                    MODULE.native_removal_preview([item]))
                self.assertEqual(planned, [item.ident])
                self.assertFalse(available)
                self.assertTrue(notes)
        self.assertEqual(
            capture_any.call_args_list[0].args[0],
            [
                "emerge", "--pretend", "--verbose", "--depclean",
                "=app-editors/nano-8.0",
            ],
        )
        self.assertEqual(
            capture_any.call_args_list[1].args[0],
            ["removepkg", "-warn", "nano"],
        )

    @patch.object(MODULE, "capture_any", return_value=(0, "dry run"))
    def test_eopkg_uses_dry_run_and_can_purge_changed_configuration(
            self, capture_any):
        item = MODULE.Match("Eopkg", "nano", "nano", scope="system")
        planned, _orphans, available, notes = (
            MODULE.native_removal_preview([item]))
        self.assertEqual(planned, ["nano"])
        self.assertFalse(available)
        self.assertIn("completed a dry-run", notes[0])
        capture_any.assert_called_once_with([
            "eopkg", "--no-color", "remove", "--dry-run", "nano",
        ])
        with patch.object(MODULE.os, "geteuid", return_value=0):
            self.assertEqual(
                MODULE.uninstall_command(item, True),
                ["eopkg", "remove", "--yes-all", "--purge", "nano"],
            )

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/swupd")
    @patch.object(MODULE, "capture_any", return_value=(
        0,
        ("os-core: installed\n"
        "desktop: explicitly installed\n"
        "graphics: installed, experimental\n"),
    ))
    def test_swupd_inventory_preserves_explicit_bundle_tracking(
            self, capture_any, _which):
        items = MODULE.detect_swupd("desktop")
        [item] = MODULE.annotate_roles(items)
        self.assertEqual((item.kind, item.ident, item.role),
                         ("Swupd", "desktop", "explicit"))
        capture_any.assert_called_once_with([
            "swupd", "bundle-list", "--status", "--quiet",
        ], timeout=30)

    @patch.object(MODULE, "swupd_inventory", return_value=(
        MODULE.PackageRecord("desktop", origin="explicit"),
        MODULE.PackageRecord("graphics", origin="dependency"),
    ))
    @patch.object(MODULE, "swupd_reverse_dependencies")
    def test_swupd_dependency_report_reaches_explicit_bundle_root(
            self, reverse, _inventory):
        reverse.side_effect = lambda target: (
            (True, {"desktop"}) if target == "graphics" else (True, set()))
        item = MODULE.Match(
            "Swupd", "graphics", "graphics", role="dependency")
        report = MODULE.swupd_dependency_report(item)
        self.assertEqual(report.direct_dependents, ["desktop"])
        self.assertEqual(report.root_paths, [["desktop", "graphics"]])
        self.assertTrue(report.complete)

    @patch.object(MODULE.os, "geteuid", return_value=0)
    def test_swupd_uses_non_force_bundle_removal(self, _euid):
        item = MODULE.Match("Swupd", "desktop", "desktop", scope="system")
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            ["swupd", "bundle-remove", "desktop"],
        )
        planned, _orphans, available, notes = (
            MODULE.native_removal_preview([item]))
        self.assertEqual(planned, ["desktop"])
        self.assertFalse(available)
        self.assertIn("does not expose a read-only", notes[0])

    @patch.object(MODULE, "portage_world", return_value={"app-editors/nano"})
    @patch.object(MODULE, "pacman_inventory_by_name", return_value={})
    @patch.object(MODULE, "command_lines", return_value=(True, {"nano"}))
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/tool")
    def test_pacman_fallback_and_portage_roles_coexist(
            self, _which, command_lines, _inventory, _world):
        pacman = MODULE.Match("Pacman", "nano", "nano")
        portage = MODULE.Match("Portage", "app-editors/nano", "nano")
        annotated = MODULE.annotate_roles([pacman, portage])
        self.assertEqual([item.role for item in annotated], ["explicit", "explicit"])
        command_lines.assert_called_once_with(["pacman", "-Qqe"])

    @patch.object(MODULE, "package_installed_size")
    def test_inventory_sizes_avoid_per_package_size_queries(self, package_size):
        matches = [
            MODULE.Match("DNF", "one", "one", size_bytes=100),
            MODULE.Match("DNF", "two", "two", size_bytes=200),
        ]
        self.assertEqual(
            [item.size_bytes for item in MODULE.add_installed_sizes(matches)],
            [100, 200],
        )
        package_size.assert_not_called()

    def test_invocation_cache_computes_a_concurrent_key_only_once(self):
        entered = Event()
        release = Event()
        calls = 0
        calls_lock = Lock()

        @MODULE.synchronized_lru_cache(maxsize=1)
        def inventory(scope):
            nonlocal calls
            with calls_lock:
                calls += 1
            entered.set()
            self.assertTrue(release.wait(timeout=2))
            return scope

        with MODULE.ThreadPoolExecutor(max_workers=2) as executor:
            first = executor.submit(inventory, "system")
            self.assertTrue(entered.wait(timeout=2))
            second = executor.submit(inventory, "system")
            release.set()
            self.assertEqual(first.result(timeout=2), "system")
            self.assertEqual(second.result(timeout=2), "system")
        self.assertEqual(calls, 1)

    def test_discovery_runs_with_four_bounded_workers(self):
        barrier = Barrier(4, timeout=2)
        active = 0
        peak = 0
        active_lock = Lock()

        def make_detector(number):
            def detector(query):
                nonlocal active, peak
                with active_lock:
                    active += 1
                    peak = max(peak, active)
                barrier.wait()
                with active_lock:
                    active -= 1
                return [MODULE.Match(
                    "Test", f"item-{number}", f"{query}-{number}")]
            return detector

        detectors = tuple(make_detector(number) for number in range(8))
        with patch.object(MODULE, "DETECTORS", detectors):
            found = MODULE.find_matches("example")
        self.assertEqual(len(found), 8)
        self.assertEqual(peak, 4)

    def test_provenance_sizes_dependency_graph_and_preview_overlap(self):
        item = MODULE.Match("Flatpak", "org.example.App", "Example")
        decoration_barrier = Barrier(2, timeout=2)

        def reasons(matches):
            decoration_barrier.wait()
            return [MODULE.replace(match, reason="requested") for match in matches]

        def sizes(matches):
            decoration_barrier.wait()
            return [MODULE.replace(match, size_bytes=1024) for match in matches]

        with patch.object(MODULE, "add_install_reasons", side_effect=reasons), \
                patch.object(MODULE, "add_installed_sizes", side_effect=sizes):
            decorated = MODULE.decorate_matches([item])
        self.assertEqual((decorated[0].reason, decorated[0].size_bytes),
                         ("requested", 1024))

        plan_barrier = Barrier(2, timeout=2)

        def report(target):
            plan_barrier.wait()
            return MODULE.DependencyReport(target, [], [], True)

        def preview(_selected):
            plan_barrier.wait()
            return [item.ident], [], True, []

        with patch.object(MODULE, "dependency_report", side_effect=report), \
                patch.object(MODULE, "native_removal_preview", side_effect=preview):
            plan = MODULE.build_removal_plan([item])
        self.assertTrue(plan.preview_available)
        self.assertEqual(plan.planned_removals, [item.ident])

    def test_command_lookup_tolerates_unambiguous_case_mistakes(self):
        with tempfile.TemporaryDirectory() as directory:
            command = Path(directory) / "dosbox"
            command.write_text("#!/bin/sh\n", encoding="utf-8")
            command.chmod(0o755)
            with patch.dict(os.environ, {"PATH": directory}):
                self.assertEqual(
                    MODULE.find_executable("DOSbox"), str(command))

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
        self.assertIn(b"--show-dependencies", result.stdout)
        self.assertNotIn(b"--why", result.stdout)
        self.assertNotIn(b"--plan", result.stdout)

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
        self.assertEqual(result[0].ident, "example.x86_64")
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
            "rpm", "-qf", "--qf",
            "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{ARCH}\\n",
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
            ('<?xml version="1.0"?><stream><package-list>'
            '<solvable kind="package" name="editor"/>'
            '</package-list></stream>'),
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

    @patch.object(MODULE, "dependency_report")
    def test_compact_reason_combines_current_cause_and_install_history(
            self, dependency_report):
        item = MODULE.Match(
            "DNF", "dosbox-staging", "dosbox-staging",
            role="weak dependency",
        )
        dependency_report.return_value = MODULE.DependencyReport(
            item, [], [], True,
            weak_relations=[("wine", "recommends")],
            relationship_paths=[
                (["wine", "dosbox-staging"], ["recommends"]),
            ],
            history_reason=(
                "DNF transaction 193: dnf install wine "
                "(recorded reason: Weak Dependency)"
            ),
        )
        self.assertEqual(
            MODULE.install_reason(item),
            "wine recommends it; DNF transaction 193: dnf install wine "
            "(recorded reason: Weak Dependency)",
        )

    def test_compact_impact_reports_extra_removals_without_verbose_plan(self):
        item = MODULE.Match("DNF", "dosbox-staging", "dosbox-staging")
        plan = MODULE.RemovalPlan(
            [item], [], ["dosbox-staging", "helper"],
            ["helper"], ["helper"], "CAUTION", True, [], [],
        )
        output = io.StringIO()
        with redirect_stdout(output):
            MODULE.show_compact_impact(plan)
        rendered = output.getvalue()
        self.assertIn(
            "Also expected to remove 1 now-unused dependency: helper",
            rendered,
        )
        self.assertNotIn("Removal preview", rendered)
        self.assertNotIn("Risk:", rendered)

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
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_dnf_history_does_not_confuse_package_name_prefixes(
            self, _which, capture):
        capture.side_effect = [
            MODULE.json.dumps([{"id": 7, "command_line": "dnf install foo"}]),
            MODULE.json.dumps([{
                "id": 7,
                "description": "dnf install foo",
                "packages": [
                    {
                        "nevra": "foo-bar-0:2.0-1.x86_64",
                        "action": "Install",
                        "reason": "Dependency",
                    },
                    {
                        "nevra": "foo-0:1.0-1.x86_64",
                        "action": "Install",
                        "reason": "User",
                    },
                ],
            }]),
        ]
        record = MODULE.dnf_install_record("foo")
        self.assertIsNotNone(record)
        self.assertEqual(record.reason, "User")

    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_dnf_history_batches_many_packages_and_transactions(
            self, capture, which):
        which.side_effect = (
            lambda command: "/usr/bin/dnf5" if command == "dnf5" else None)

        def output(command):
            if command[:3] == ["dnf5", "history", "list"]:
                return MODULE.json.dumps([
                    {"id": 7, "command_line": "dnf install alpha-tool"},
                    {"id": 9, "command_line": "dnf install beta"},
                ])
            if command[:3] == ["dnf5", "history", "info"]:
                return MODULE.json.dumps([
                    {
                        "id": 7,
                        "description": "dnf install alpha-tool",
                        "packages": [{
                            "nevra": "alpha-tool-0:1.0-1.x86_64",
                            "action": "Install",
                            "reason": "User",
                        }],
                    },
                    {
                        "id": 9,
                        "description": "dnf install beta",
                        "packages": [{
                            "nevra": "beta-0:2.0-1.x86_64",
                            "action": "Install",
                            "reason": "Weak Dependency",
                        }],
                    },
                ])
            self.fail(f"unexpected command: {command}")

        capture.side_effect = output
        MODULE.prefetch_dnf_install_records(("alpha-tool", "beta"))

        self.assertEqual(
            MODULE.dnf_install_record("alpha-tool").transaction_id, 7)
        self.assertEqual(
            MODULE.dnf_install_record("beta").reason, "Weak Dependency")
        self.assertEqual(capture.call_count, 2)
        self.assertEqual(
            capture.call_args_list[0].args[0],
            [
                "dnf5", "history", "list",
                "--contains-pkgs=alpha-tool,beta", "--json",
            ],
        )
        self.assertEqual(
            capture.call_args_list[1].args[0],
            ["dnf5", "history", "info", "7", "9", "--json"],
        )

    @patch.object(
        MODULE, "dnf_environment_details",
        return_value=(
            "COSMIC Desktop",
            ("cosmic-desktop", "cosmic-desktop-apps"),
        ),
    )
    @patch.object(
        MODULE, "dnf_group_memberships",
        return_value=((
            "cosmic-desktop-apps",
            "COSMIC Desktop Supplementary Applications",
        ),),
    )
    @patch.object(MODULE, "dnf_install_record")
    def test_dnf_group_reason_names_environment_group_and_transaction(
            self, install_record, _memberships, _environment):
        install_record.return_value = MODULE.DnfInstallRecord(
            224,
            "dnf install @cosmic-desktop-environment",
            "Group",
            ("cosmic-desktop", "cosmic-desktop-apps"),
            ("cosmic-desktop-environment",),
        )
        item = MODULE.Match(
            "DNF", "okular", "okular", role="group")
        self.assertEqual(
            MODULE.install_reason(item),
            "installed through COSMIC Desktop Environment \u2192 "
            "COSMIC Desktop Supplementary Applications "
            "(cosmic-desktop-apps); DNF transaction 224: "
            "dnf install @cosmic-desktop-environment",
        )

    @patch.object(MODULE, "capture")
    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/dnf5")
    def test_dnf_group_membership_uses_installed_cache_only_metadata(
            self, _which, capture):
        capture.return_value = (
            "Id : cosmic-desktop-apps\n"
            "Name : COSMIC Desktop Supplementary Applications\n"
            "Installed : yes\n"
            "Default packages : okular\n"
            "                 : other-app\n"
        )
        self.assertEqual(
            MODULE.dnf_group_memberships("okular"),
            ((
                "cosmic-desktop-apps",
                "COSMIC Desktop Supplementary Applications",
            ),),
        )
        capture.assert_called_once_with([
            "dnf5", "-q", "-C", "group", "info",
            "--installed", "--hidden",
        ])

    def test_long_history_command_keeps_decisive_group_target(self):
        command = (
            "dnf5 --config /etc/dnf/dnf.conf --installroot / "
            "install @core @workstation-product-environment "
            + " ".join(f"package-{number}" for number in range(30))
        )
        self.assertEqual(
            MODULE.compact_history_command(command),
            "dnf5 install @workstation-product-environment "
            "(original command abbreviated)",
        )

    def test_short_history_command_is_not_changed(self):
        self.assertEqual(
            MODULE.compact_history_command("dnf install wine"),
            "dnf install wine",
        )

    @patch.object(
        MODULE, "dnf_group_memberships",
        return_value=(
            ("first", "First Group"),
            ("second", "Second Group"),
        ),
    )
    @patch.object(MODULE, "dnf_install_record")
    def test_ambiguous_dnf_group_membership_falls_back_without_guessing(
            self, install_record, _memberships):
        install_record.return_value = MODULE.DnfInstallRecord(
            9, "dnf group install desktop", "Group",
            ("first", "second"), (),
        )
        self.assertEqual(
            MODULE.dnf_group_install_reason("example"),
            "installed as part of a package group; "
            "DNF transaction 9: dnf group install desktop",
        )

    @patch.object(MODULE, "dnf_install_record")
    def test_explicit_dnf_reason_includes_repository_and_transaction(
            self, install_record):
        install_record.return_value = MODULE.DnfInstallRecord(
            114, "dnf install yazi", "User",
            repository="copr:owner:yazi",
        )
        item = MODULE.Match("DNF", "yazi", "yazi", role="explicit")
        self.assertEqual(
            MODULE.install_reason(item),
            "explicitly installed from copr:owner:yazi; "
            "DNF transaction 114: dnf install yazi",
        )

    @patch.object(
        MODULE, "rpm_install_metadata",
        return_value=("Tue Jul 14 17:40:05 2026", "Example Vendor"),
    )
    def test_plain_rpm_reason_discloses_database_limitations(self, _metadata):
        item = MODULE.Match("RPM", "example", "example", role="unknown")
        self.assertEqual(
            MODULE.install_reason(item),
            "RPM database install time Tue Jul 14 17:40:05 2026; "
            "package vendor Example Vendor; RPM does not record whether it "
            "was explicitly requested; original command and source unavailable",
        )

    def test_apt_history_parser_finds_the_original_command(self):
        text = (
            "Start-Date: 2026-07-01  10:20:00\n"
            "Commandline: apt install freecad\n"
            "Install: freecad:amd64 (1.0, automatic), "
            "libfoo:amd64 (2.0, automatic)\n"
            "End-Date: 2026-07-01  10:20:05\n"
        )
        self.assertEqual(
            MODULE.parse_apt_history(text, "freecad"),
            [("2026-07-01  10:20:00", "apt install freecad")],
        )

    @patch.object(
        MODULE, "apt_history_event",
        return_value=("2026-07-01 10:20:00", "apt install freecad"),
    )
    def test_explicit_apt_reason_distinguishes_state_from_history(
            self, _history):
        item = MODULE.Match("APT", "freecad", "freecad", role="explicit")
        self.assertEqual(
            MODULE.install_reason(item),
            "marked manually installed by APT; "
            "APT history on 2026-07-01 10:20:00: apt install freecad",
        )

    def test_pacman_history_parser_connects_command_to_install(self):
        text = (
            "[2026-07-01T10:00:00+0000] [PACMAN] "
            "Running 'pacman -S okular'\n"
            "[2026-07-01T10:00:01+0000] [ALPM] "
            "installed okular (1.0-1)\n"
        )
        self.assertEqual(
            MODULE.parse_pacman_history(text, "okular"),
            ("2026-07-01T10:00:01+0000", "pacman -S okular"),
        )

    def test_zypper_history_parser_reports_recorded_repository(self):
        text = (
            "2026-07-01 10:00:00|install|okular|1.0|x86_64|"
            "root@host|repo-oss|checksum|\n"
        )
        self.assertEqual(
            MODULE.parse_zypper_history(text, "okular"),
            ("2026-07-01 10:00:00", "installed from repo-oss"),
        )

    def test_each_history_file_is_parsed_once_for_many_packages(self):
        cases = (
            (
                MODULE.apt_history_index,
                MODULE.apt_history_event,
                ("Start-Date: 2026-07-01  10:20:00\n"
                "Commandline: apt install alpha beta\n"
                "Install: alpha:amd64 (1.0), beta:amd64 (2.0)\n"),
                ("alpha", "beta"),
            ),
            (
                MODULE.pacman_history_index,
                MODULE.pacman_history_event,
                ("[2026-07-01T10:00:00+0000] [PACMAN] "
                "Running 'pacman -S alpha beta'\n"
                "[2026-07-01T10:00:01+0000] [ALPM] installed alpha (1-1)\n"
                "[2026-07-01T10:00:02+0000] [ALPM] installed beta (1-1)\n"),
                ("alpha", "beta"),
            ),
            (
                MODULE.zypper_history_index,
                MODULE.zypper_history_event,
                ("2026-07-01 10:00:00|install|alpha|1|x86_64|root|repo-a|x|\n"
                "2026-07-01 10:00:01|install|beta|1|x86_64|root|repo-b|x|\n"),
                ("alpha", "beta"),
            ),
        )
        fake_log = Path("/var/log/fake-history")
        for index_function, event_function, text, packages in cases:
            with self.subTest(index=index_function.__name__):
                index_function.cache_clear()
                event_function.cache_clear()
                with patch.object(
                        MODULE.Path, "glob", return_value=[fake_log]), \
                        patch.object(
                            MODULE, "read_history_file",
                            return_value=text) as read:
                    self.assertIsNotNone(event_function(packages[0]))
                    self.assertIsNotNone(event_function(packages[1]))
                read.assert_called_once_with(fake_log)

    def test_legacy_dnf_history_parser_recovers_original_command(self):
        text = (
            "Transaction ID : 42\n"
            "Command Line : dnf install okular\n"
            "Packages Altered:\n"
            "    Install okular-1.0-1.x86_64 @updates\n"
        )
        self.assertEqual(
            MODULE.parse_legacy_rpm_history_info(text, "okular"),
            "dnf install okular",
        )

    @patch.object(MODULE, "capture")
    def test_flatpak_evidence_includes_remote_and_install_event(self, capture):
        capture.side_effect = [
            "org.freecad.FreeCAD\tFreeCAD\t1.0\tflathub\n",
            MODULE.json.dumps([
                {
                    "time": "Jul 1 10:00:00",
                    "change": "deploy install",
                    "application": "org.freecad.FreeCAD",
                    "installation": "system",
                    "remote": "flathub",
                },
            ]),
        ]
        self.assertEqual(
            MODULE.flatpak_install_evidence(
                "org.freecad.FreeCAD", "system"),
            (
                "flathub",
                ("Flatpak history Jul 1 10:00:00: "
                "install org.freecad.FreeCAD"),
            ),
        )

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/flatpak")
    @patch.object(MODULE, "capture")
    def test_flatpak_search_and_provenance_share_inventory_origin(
            self, capture, _which):
        def output(command):
            if command[:3] == ["flatpak", "list", "--app"]:
                if "--system" in command:
                    return "org.example.App\tExample App\t1.0\tflathub\n"
                return ""
            if command[:2] == ["flatpak", "history"]:
                return "[]"
            self.fail(f"unexpected command: {command}")

        capture.side_effect = output
        [item] = MODULE.detect_flatpak("example app")
        remote, _event = MODULE.flatpak_install_evidence(
            item.ident, item.scope)

        self.assertEqual(remote, "flathub")
        self.assertFalse(any(
            command.args[0][:2] == ["flatpak", "info"]
            for command in capture.call_args_list
        ))
        self.assertEqual(sum(
            command.args[0][:3] == ["flatpak", "list", "--app"]
            for command in capture.call_args_list
        ), 2)

    @patch.object(MODULE, "capture")
    def test_snap_evidence_includes_channel_publisher_and_change(
            self, capture):
        capture.side_effect = [
            "tracking: latest/stable\npublisher: KDE\u2713\n",
            ("ID Status Spawn Ready Summary\n"
            "84 Done now now Install \"okular\" snap\n"),
        ]
        self.assertEqual(
            MODULE.snap_install_evidence("okular"),
            ("latest/stable", "KDE\u2713", 'snap change 84: Install "okular" snap'),
        )

    @patch.object(MODULE, "capture")
    def test_homebrew_evidence_uses_tap_and_requested_state(self, capture):
        capture.return_value = MODULE.json.dumps({
            "formulae": [{
                "tap": "homebrew/core",
                "installed": [{"installed_on_request": True}],
            }],
        })
        self.assertEqual(
            MODULE.homebrew_install_evidence("Homebrew", "ripgrep"),
            ("homebrew/core", True),
        )

    @patch.object(MODULE.shutil, "which", return_value="/opt/homebrew/bin/brew")
    @patch.object(MODULE, "capture")
    def test_homebrew_search_role_and_source_use_one_json_inventory(
            self, capture, _which):
        capture.return_value = MODULE.json.dumps({
            "formulae": [{
                "name": "ripgrep",
                "tap": "homebrew/core",
                "installed": [{
                    "version": "14.1.1",
                    "installed_on_request": True,
                }],
            }],
            "casks": [],
        })
        [item] = MODULE.detect_homebrew("ripgrep")
        [annotated] = MODULE.annotate_roles([item])
        reason = MODULE.install_reason(annotated)

        self.assertEqual((annotated.version, annotated.role),
                         ("14.1.1", "explicit"))
        self.assertIn("homebrew/core", reason)
        capture.assert_called_once_with([
            "brew", "info", "--json=v2", "--installed",
        ])

    @patch.object(MODULE, "nix_profile_metadata")
    def test_nix_reason_uses_original_flake_reference(self, metadata):
        metadata.return_value = {
            "ripgrep": {
                "originalUrl": "github:NixOS/nixpkgs",
                "attrPath": "legacyPackages.x86_64-linux.ripgrep",
            },
        }
        item = MODULE.Match("Nix", "ripgrep", "ripgrep", role="explicit")
        rendered = MODULE.install_reason(item)
        self.assertIn("github:NixOS/nixpkgs#legacyPackages", rendered)
        self.assertIn("current Nix profile", rendered)

    @patch.object(
        MODULE, "cargo_install_source",
        return_value=(
            "registry+https://github.com/rust-lang/crates.io-index"),
    )
    def test_cargo_reason_names_recorded_registry_source(self, _source):
        item = MODULE.Match(
            "Cargo", "ripgrep", "ripgrep", role="explicit")
        self.assertIn(
            "crates.io (recorded source registry+https://",
            MODULE.install_reason(item),
        )

    @patch.object(MODULE, "pipx_install_source", return_value="httpie")
    def test_pipx_reason_names_package_and_environment(self, _source):
        item = MODULE.Match(
            "Pipx", "httpie", "httpie", scope="user", role="explicit")
        self.assertEqual(
            MODULE.install_reason(item),
            "explicitly installed with pipx from PyPI package httpie; "
            "environment httpie",
        )

    @patch.object(
        MODULE, "npm_install_source",
        return_value="https://registry.npmjs.org/tool/-/tool-2.0.0.tgz",
    )
    def test_npm_reason_uses_resolved_package_source(self, _source):
        item = MODULE.Match("NPM", "tool", "tool", role="explicit")
        self.assertIn(
            "top-level global npm package resolved from "
            "https://registry.npmjs.org",
            MODULE.install_reason(item),
        )

    def test_file_based_installers_are_explicit_about_missing_history(self):
        appimage = MODULE.Match(
            "AppImage", "/apps/Tool.AppImage", "Tool",
            path=Path("/apps/Tool.AppImage"), role="explicit")
        standalone = MODULE.Match(
            "Standalone", "/usr/local/bin/tool", "tool",
            path=Path("/usr/local/bin/tool"), role="explicit")
        gearlever = MODULE.Match(
            "Gear Lever", "/apps/Tool.AppImage", "Tool",
            path=Path("/apps/Tool.AppImage"), role="explicit",
            origin="https://example.com/releases",
        )
        self.assertIn("no package manager", MODULE.install_reason(appimage))
        self.assertIn("original source is unknown",
                      MODULE.install_reason(standalone))
        self.assertIn("update source https://example.com/releases",
                      MODULE.install_reason(gearlever))

    @patch.object(
        MODULE, "explicit_install_reason",
        return_value="marked manually installed by APT; history unavailable",
    )
    def test_matching_archive_is_not_claimed_as_original_source(
            self, _reason):
        item = MODULE.Match(
            "APT", "example", "example", role="explicit",
            archive=Path("/tmp/example.deb"),
        )
        self.assertIn(
            "matches this installed package but is not proven",
            MODULE.install_reason(item),
        )

    def test_multiple_dependency_roots_are_summarized_on_one_line(self):
        item = MODULE.Match("APT", "libfoo", "libfoo", role="dependency")
        report = MODULE.DependencyReport(
            item, [], [
                ["texlive", "tex-engine", "libfoo"],
                ["wine", "wine-core", "libfoo"],
                ["third-app", "helper", "libfoo"],
            ],
        )
        rendered = MODULE.compact_dependency_cause(report)
        self.assertIn("texlive -> tex-engine -> libfoo", rendered)
        self.assertIn("wine -> wine-core -> libfoo", rendered)
        self.assertIn("(+1 other roots)", rendered)

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
            ("Removing:\n target x86_64 1 repo 1 MiB\n\n"
            "Transaction Summary:\nOperation aborted by the user.\n"),
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
    @patch.object(MODULE, "find_user_data", return_value=[])
    @patch.object(MODULE, "build_removal_plan")
    @patch.object(MODULE, "filter_dependency_matches")
    @patch.object(MODULE, "annotate_roles")
    @patch.object(MODULE, "find_matches")
    def test_unknown_single_item_confirms_with_displayed_name_not_path(
            self, find_matches, annotate, filter_matches, build_plan,
            _find_data, run):
        standalone_path = Path.home() / ".local/bin/edit"
        item = MODULE.Match(
            "Standalone", str(standalone_path), "edit",
            scope="user", path=standalone_path,
            role="explicit",
        )
        find_matches.return_value = [item]
        annotate.return_value = [item]
        filter_matches.return_value = ([item], 0)
        build_plan.return_value = MODULE.RemovalPlan(
            [item], [MODULE.DependencyReport(item, [], [])],
            [item.ident], [], [], "UNKNOWN", False, [], [])
        run.return_value.returncode = 0
        with patch.object(MODULE, "preflight_file", return_value=""), \
                patch.object(
                    MODULE, "privileged",
                    side_effect=lambda command: command,
                ), \
                patch("builtins.input", return_value="REMOVE edit") as prompt:
            self.assertEqual(MODULE.run_uninstall("edit"), 0)
        self.assertIn("Type 'REMOVE edit'", prompt.call_args.args[0])
        run.assert_called_once_with(
            ["rm", "--", str(standalone_path)], check=False)

    def test_removed_read_only_flags_are_rejected(self):
        for flag in ("--why", "--plan"):
            with self.subTest(flag=flag):
                result = subprocess.run(
                    [str(SCRIPT), flag, "tool"],
                    check=False, capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn("unrecognized arguments", result.stderr)

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
    def test_non_node_command_does_not_query_the_npm_prefix(
            self, capture, which):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "edit"
            executable.write_text("#!/bin/sh\n", encoding="utf-8")
            executable.chmod(0o755)
            which.side_effect = {
                "edit": str(executable),
                "npm": "/usr/bin/npm",
            }.get
            MODULE.detect_executable_owner("edit")
        capture.assert_not_called()

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

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/pipx")
    @patch.object(MODULE, "capture")
    def test_pipx_source_reuses_the_discovery_inventory(
            self, capture, _which):
        capture.return_value = MODULE.json.dumps({
            "venvs": {
                "httpie": {
                    "metadata": {
                        "main_package": {
                            "package": "httpie",
                            "package_version": "3.2.4",
                            "package_or_url": "https://example.test/httpie.whl",
                        },
                    },
                },
            },
        })
        [item] = MODULE.pipx_inventory("user")
        self.assertEqual(
            MODULE.pipx_install_source(item.ident, item.scope),
            "https://example.test/httpie.whl",
        )
        capture.assert_called_once_with(["pipx", "list", "--json"])

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

    @patch.object(MODULE.shutil, "which")
    def test_cargo_search_and_source_share_local_install_metadata(self, which):
        with tempfile.TemporaryDirectory() as directory:
            cargo_root = Path(directory)
            (cargo_root / ".crates2.json").write_text(
                MODULE.json.dumps({
                    "installs": {
                        "ripgrep 14.1.1 (registry+https://example.test/index)": {
                            "bins": ["rg"],
                        },
                    },
                }),
                encoding="utf-8",
            )
            which.side_effect = {
                "cargo": "/usr/bin/cargo",
                "rg": str(cargo_root / "bin/rg"),
            }.get
            with patch.dict(os.environ, {"CARGO_HOME": str(cargo_root)}):
                [item] = MODULE.detect_cargo("rg")
                source = MODULE.cargo_install_source(item.ident)

        self.assertEqual((item.ident, item.version), ("ripgrep", "14.1.1"))
        self.assertEqual(source, "registry+https://example.test/index")

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

    @patch.object(MODULE.Path, "home", return_value=Path("/home/test"))
    @patch.object(MODULE.shutil, "which")
    @patch.object(MODULE, "capture")
    def test_guix_profile_entries_are_discovered_and_removed_transactionally(
            self, capture, which, _home):
        which.side_effect = {
            "guix": "/gnu/store/current/bin/guix",
        }.get
        capture.return_value = (
            "emacs\t29.4\tout\t/gnu/store/hash-emacs-29.4\n")

        [item] = MODULE.detect_guix("emacs")

        self.assertEqual(
            (item.kind, item.ident, item.version, item.origin),
            ("Guix", "emacs", "29.4", "/home/test/.guix-profile"),
        )
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            [
                "guix", "package", "--profile=/home/test/.guix-profile",
                "--remove=emacs",
            ],
        )
        capture.assert_called_once_with([
            "guix", "package", "--profile=/home/test/.guix-profile",
            "--list-installed",
        ])

    @patch.object(MODULE.shutil, "which", return_value="/usr/bin/nix-env")
    @patch.object(MODULE, "capture", return_value=(
        "hello-2.12.1 /nix/store/hash-hello-2.12.1\n"))
    def test_legacy_nix_environment_is_supported(self, capture, _which):
        [record] = MODULE.nix_env_inventory()
        item = MODULE.Match(
            "Nix Legacy", record.ident, record.ident, record.version,
            "user", origin=record.origin, role="explicit",
        )
        self.assertEqual((record.ident, record.version), ("hello", "2.12.1"))
        self.assertIn("legacy Nix user profile", MODULE.install_reason(item))
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            ["nix-env", "--uninstall", "hello"],
        )
        capture.assert_called_once_with([
            "nix-env", "--query", "--installed", "--out-path",
        ])

    @patch.object(MODULE.shutil, "which", side_effect=(
        lambda command: "/usr/bin/nix-env" if command == "nix-env" else None))
    @patch.object(MODULE, "nix_env_inventory", return_value=(
        MODULE.PackageRecord(
            "hello", "2.12.1", origin="/nix/store/hash-hello-2.12.1"),))
    def test_legacy_nix_detection_does_not_require_modern_nix_command(
            self, _inventory, _which):
        [item] = MODULE.detect_nix("hello")
        self.assertEqual((item.kind, item.ident), ("Nix Legacy", "hello"))

    @patch.object(MODULE.os, "geteuid", return_value=0)
    @patch.object(MODULE, "transactional_zypper_system", return_value=True)
    def test_read_only_suse_removal_uses_transactional_update(
            self, _transactional, _euid):
        item = MODULE.Match("Zypper", "nano", "nano", scope="system")
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            [
                "transactional-update", "--non-interactive", "pkg", "remove",
                "nano",
            ],
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
        with patch("builtins.input", side_effect=["a", "REMOVE Example"]), \
                patch.object(MODULE, "revalidate_package_identity", return_value=""):
            self.assertEqual(MODULE.run_uninstall("Example"), 1)
        remove_paths.assert_not_called()
        self.assertTrue(any(
            "--delete-data" in recorded.args[0]
            for recorded in run.call_args_list
        ))

    def test_user_can_choose_individual_cleanup_paths(self):
        paths = [Path("/tmp/first"), Path("/tmp/second")]
        with patch("builtins.input", return_value="2"):
            cleanup_kinds, selected_paths = MODULE.ask_cleanup([], paths)
        self.assertEqual(cleanup_kinds, set())
        self.assertEqual(selected_paths, [paths[1]])

    def test_managed_and_detected_data_share_one_numbered_prompt(self):
        selected = [
            MODULE.Match(
                "Flatpak", "org.freecad.FreeCAD", "FreeCAD", scope="system"),
        ]
        paths = [
            Path("/home/test/.cache/FreeCAD"),
            Path("/home/test/.config/FreeCAD"),
        ]
        output = io.StringIO()
        with patch.object(
                MODULE, "manager_cleanup_size",
                return_value=512 * 1024 * 1024), \
                patch.object(
                    MODULE, "path_disk_sizes",
                    return_value={
                        paths[0].absolute(): 184 * 1024 * 1024,
                        paths[1].absolute(): int(2.3 * 1024 * 1024),
                    }), \
                redirect_stdout(output), \
                patch("builtins.input", return_value="1,3"):
            cleanup_kinds, selected_paths = MODULE.ask_cleanup(selected, paths)
        self.assertEqual(cleanup_kinds, {"Flatpak"})
        self.assertEqual(selected_paths, [paths[1]])
        rendered = output.getvalue()
        self.assertIn("Remove associated data too? (optional)", rendered)
        self.assertIn("[Flatpak] Sandbox data and permissions", rendered)
        self.assertIn("[Detected] /home/test/.cache/FreeCAD", rendered)
        self.assertIn("512 MiB", rendered)
        self.assertIn("184 MiB", rendered)
        self.assertIn("2.3 MiB", rendered)
        self.assertIn("Flatpak data is manager-owned.", rendered)
        self.assertIn("not guaranteed to belong to this app", rendered)

    def test_cleanup_summary_uses_recursive_removal_for_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "FreeCAD"
            path.mkdir()
            (path / "settings").touch()
            self.assertEqual(
                MODULE.cleanup_display_command(path),
                ["rm", "-r", "--", str(path)],
            )

    @patch.object(MODULE, "path_disk_size", return_value=128 * 1024 * 1024)
    @patch.object(
        MODULE, "package_installed_size",
        return_value=256 * 1024 * 1024,
    )
    def test_space_estimate_includes_app_dependencies_and_selected_data(
            self, _package_size, _path_size):
        item = MODULE.Match(
            "DNF", "app", "app", size_bytes=1024 * 1024 * 1024)
        plan = MODULE.RemovalPlan(
            [item], [], ["app", "helper"], ["helper"], ["helper"],
            "CAUTION", True, [], [],
        )
        total, complete = MODULE.removal_space_estimate(
            plan, set(), [Path("/home/test/.config/app")])
        self.assertTrue(complete)
        self.assertEqual(total, 1408 * 1024 * 1024)
        self.assertEqual(
            MODULE.ready_heading(total, complete),
            "Ready to run (estimated installed data affected: about 1.4 GiB):",
        )

    def test_unknown_sizes_are_disclosed_in_ready_heading(self):
        self.assertEqual(
            MODULE.ready_heading(1024 * 1024, False),
            "Ready to run (at least 1 MiB installed data affected; "
            "some sizes and reclaimed space unknown):",
        )
        self.assertEqual(
            MODULE.ready_heading(0, False),
            "Ready to run (affected size and reclaimed space unknown):",
        )

    def test_manager_cleanup_choices_are_independent(self):
        selected = [
            MODULE.Match(
                "Flatpak", "org.example.App", "Example", scope="user"),
            MODULE.Match("APT", "example", "example"),
        ]
        with patch("builtins.input", return_value="1"):
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
    @patch.object(MODULE.shutil, "which")
    def test_pkexec_is_the_final_privilege_fallback(self, which, _euid):
        which.side_effect = {"pkexec": "/usr/bin/pkexec"}.get
        self.assertEqual(
            MODULE.privileged(["apk", "del", "thing"]),
            ["pkexec", "apk", "del", "thing"],
        )

    @patch.object(MODULE.os, "geteuid", return_value=1000)
    @patch.object(MODULE.shutil, "which", return_value=None)
    def test_missing_privilege_helper_fails_before_removal(self, _which, _euid):
        with self.assertRaisesRegex(RuntimeError, "sudo, doas, and pkexec"):
            MODULE.privileged(["dnf", "remove", "thing"])

    def test_selection_rejects_out_of_range_input(self):
        matches = [MODULE.Match("APT", "freecad", "freecad")]
        with patch("builtins.input", side_effect=["2", ""]):
            self.assertEqual(MODULE.choose(matches), [])

    def test_single_result_is_auto_selected(self):
        item = MODULE.Match("DNF", "dosbox-staging", "dosbox-staging")
        with patch("builtins.input", side_effect=AssertionError(
                "read-only unique result must not prompt")):
            self.assertEqual(
                MODULE.choose([item], auto_select=True),
                [item],
            )

    def test_standalone_result_uses_concise_related_files_note(self):
        item = MODULE.Match(
            "Standalone", "/home/test/.local/bin/edit", "edit")
        output = io.StringIO()
        with redirect_stdout(output):
            MODULE.choose([item], auto_select=True)
        self.assertIn(
            "note: related files cannot be identified automatically",
            output.getvalue(),
        )
        self.assertNotIn("only this executable is known", output.getvalue())

    @patch.object(MODULE, "self_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall"])
    def test_uninstall_uninstall_is_self_uninstall(self, self_uninstall):
        self.assertEqual(MODULE.main(), 0)
        self_uninstall.assert_called_once_with()

    def test_apt_history_resets_origin_after_remove_and_reinstall(self):
        history = (
            "Start-Date: 2025-01-01 10:00:00\n"
            "Commandline: apt install wine\n"
            "Install: dosbox:amd64 (1.0)\n\n"
            "Start-Date: 2025-02-01 10:00:00\n"
            "Commandline: apt remove dosbox\n"
            "Remove: dosbox:amd64 (1.0)\n\n"
            "Start-Date: 2025-03-01 10:00:00\n"
            "Commandline: apt install dosbox\n"
            "Install: dosbox:amd64 (1.0)\n"
        )
        with patch.object(MODULE.Path, "glob", return_value=[Path("history")]), \
                patch.object(MODULE, "read_history_file", return_value=history):
            self.assertEqual(
                MODULE.apt_history_index()["dosbox"][1],
                "apt install dosbox",
            )

    def test_pacman_history_resets_origin_after_removal(self):
        history = (
            "[2025-01-01] [PACMAN] Running 'pacman -S wine'\n"
            "[2025-01-01] [ALPM] installed dosbox (1.0)\n"
            "[2025-02-01] [ALPM] removed dosbox (1.0)\n"
            "[2025-03-01] [PACMAN] Running 'pacman -S dosbox'\n"
            "[2025-03-01] [ALPM] installed dosbox (1.0)\n"
        )
        with patch.object(MODULE.Path, "glob", return_value=[Path("pacman.log")]), \
                patch.object(MODULE, "read_history_file", return_value=history):
            self.assertEqual(
                MODULE.pacman_history_index()["dosbox"][1],
                "pacman -S dosbox",
            )

    @patch.object(MODULE, "capture_any", return_value=(
        0, ("World updated, but the following packages are not removed due to:\n"
        "  pcre2: git requires pcre2\n"),
    ))
    def test_apk_successful_noop_is_blocked_not_planned(self, _capture):
        item = MODULE.Match("APK", "pcre2", "pcre2", scope="system")
        preview = MODULE.native_removal_preview([item])
        self.assertEqual(preview.status, MODULE.PreviewStatus.BLOCKED)
        self.assertEqual(preview.planned, ())
        plan = MODULE.build_removal_plan([item])
        self.assertEqual(plan.level, "BLOCKED")
        self.assertEqual(plan.planned_removals, [])

    def test_broad_xdg_system_root_is_rejected(self):
        with patch.dict(os.environ, {"XDG_CONFIG_HOME": "/etc"}), \
                patch.object(MODULE.Path, "home", return_value=Path("/home/test")):
            self.assertEqual(
                MODULE.xdg_dir("XDG_CONFIG_HOME", Path("/home/test/.config")),
                Path("/home/test/.config"),
            )

    def test_cleanup_refuses_a_replaced_selected_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate = root / "Example"
            candidate.mkdir()
            snapshot = MODULE.snapshot_cleanup_candidate(candidate)
            self.assertIsNotNone(snapshot)
            MODULE._CLEANUP_SNAPSHOTS[candidate.absolute()] = snapshot
            candidate.rmdir()
            candidate.mkdir()
            with patch.object(MODULE, "user_data_roots", return_value=[root]):
                self.assertFalse(MODULE.remove_paths([candidate]))
            self.assertTrue(candidate.exists())

    def test_named_flatpak_installation_is_preserved_in_command(self):
        item = MODULE.Match(
            "Flatpak", "org.example.App", "Example", scope="system",
            installation="work",
        )
        self.assertEqual(
            MODULE.uninstall_command(item, False),
            ["flatpak", "uninstall", "-y", "--installation=work",
             "org.example.App"],
        )

    def test_cleanup_choice_changes_apt_preview_to_purge(self):
        item = MODULE.Match("APT", "example", "example", scope="system")
        with patch.object(MODULE, "capture_any", return_value=(
                0, "Remv example [1.0]\n")) as capture:
            preview = MODULE.native_removal_preview([item], {"APT"})
        self.assertEqual(preview.status, MODULE.PreviewStatus.EXACT)
        capture.assert_called_once_with([
            "apt-get", "--simulate", "purge", "example",
        ])

    def test_zypper_xml_preview_is_exact_and_preserves_architecture(self):
        output = (
            "<?xml version='1.0'?><stream><install-summary>"
            "<to-remove>"
            "<solvable type='package' name='ed' arch='x86_64'/>"
            "<solvable type='package' name='helper' arch='noarch'/>"
            "</to-remove></install-summary></stream>"
        )
        item = MODULE.Match("Zypper", "ed.x86_64", "ed", scope="system")
        with patch.object(MODULE, "capture_any", return_value=(0, output)) \
                as capture:
            preview = MODULE.native_removal_preview([item])
        self.assertEqual(preview.status, MODULE.PreviewStatus.EXACT)
        self.assertEqual(preview.planned, ("ed.x86_64", "helper.noarch"))
        capture.assert_called_once_with([
            "zypper", "--xmlout", "--non-interactive",
            "remove", "--dry-run", "ed.x86_64",
        ])

    def test_unknown_preview_takes_precedence_over_dependency_caution(self):
        item = MODULE.Match(
            "Zypper", "example.x86_64", "example",
            role="dependency", scope="system",
        )
        preview = MODULE.PreviewResult(
            MODULE.PreviewStatus.UNKNOWN,
            ("example.x86_64",), fingerprint="preview",
        )
        report = MODULE.DependencyReport(item, [], [], True)
        with patch.object(MODULE, "native_removal_preview", return_value=preview), \
                patch.object(MODULE, "dependency_report", return_value=report):
            plan = MODULE.build_removal_plan([item])
        self.assertEqual(plan.level, "UNKNOWN")

    def test_rpm_multiarch_matches_keep_exact_architecture(self):
        records = (
            MODULE.RpmPackageRecord(
                "library", "1-1", "Library", 1000, "", "",
                architecture="x86_64"),
            MODULE.RpmPackageRecord(
                "library", "1-1", "Library", 900, "", "",
                architecture="i686"),
        )
        with patch.object(MODULE.shutil, "which", return_value="/usr/bin/rpm"), \
                patch.object(MODULE, "rpm_manager", return_value="DNF"), \
                patch.object(MODULE, "rpm_inventory", return_value=records):
            found = MODULE.detect_rpm("library")
        self.assertEqual(
            {item.ident for item in found},
            {"library.x86_64", "library.i686"},
        )
        self.assertEqual(
            {item.architecture for item in found}, {"x86_64", "i686"})

    def test_rpm_exact_multiarch_identifier_selects_only_that_architecture(self):
        records = (
            MODULE.RpmPackageRecord(
                "library", "1-1", "Library", 1000, "", "",
                architecture="x86_64"),
            MODULE.RpmPackageRecord(
                "library", "1-1", "Library", 900, "", "",
                architecture="i686"),
        )
        with patch.object(MODULE.shutil, "which", return_value="/usr/bin/rpm"), \
                patch.object(MODULE, "rpm_manager", return_value="DNF"), \
                patch.object(MODULE, "rpm_inventory", return_value=records):
            found = MODULE.detect_rpm("library.i686")
        self.assertEqual([item.ident for item in found], ["library.i686"])

    def test_pinning_preserves_dispatch_symlink_name(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "rustup"
            target.write_text("", encoding="utf-8")
            target.chmod(0o755)
            cargo = root / "cargo"
            cargo.symlink_to(target)
            with patch.object(MODULE.shutil, "which", return_value=str(cargo)):
                self.assertEqual(
                    MODULE.pinned_command(["cargo", "uninstall", "tool"])[0],
                    str(cargo),
                )

    def test_noninteractive_mode_refuses_unknown_preview(self):
        item = MODULE.Match("Standalone", "/tmp/tool", "tool")
        plan = MODULE.RemovalPlan(
            [item], [], [item.ident], [], [], "UNKNOWN", False, [], [],
            MODULE.PreviewStatus.UNSUPPORTED.value, "fingerprint", [],
        )
        with patch.object(MODULE, "find_matches", return_value=[item]), \
                patch.object(MODULE, "annotate_roles", return_value=[item]), \
                patch.object(MODULE, "decorate_matches", return_value=[item]), \
                patch.object(MODULE, "build_removal_plan", return_value=plan), \
                patch.object(MODULE.subprocess, "run") as run:
            result = MODULE.exact_noninteractive_remove(
                "/tmp/tool", "Standalone", "REMOVE Standalone:/tmp/tool")
        self.assertEqual(result, 1)
        run.assert_not_called()

    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "uninstall-helper"])
    def test_longer_name_remains_a_normal_search(self, run_uninstall):
        self.assertEqual(MODULE.main(), 0)
        run_uninstall.assert_called_once_with(
            "uninstall-helper", show_dependencies=False,
        )

    @patch.object(MODULE, "run_uninstall", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall"])
    def test_no_argument_prompts_for_an_app(self, run_uninstall):
        with patch("builtins.input", return_value="DOSbox"):
            self.assertEqual(MODULE.main(), 0)
        run_uninstall.assert_called_once_with(
            "DOSbox", show_dependencies=False,
        )

    @patch.object(MODULE.os, "geteuid", return_value=0)
    @patch.object(MODULE.sys, "argv", ["uninstall", "freecad"])
    def test_running_whole_program_through_sudo_is_refused(self, _euid):
        with patch.dict(os.environ, {"SUDO_USER": "test"}, clear=False):
            self.assertEqual(MODULE.main(), 2)


if __name__ == "__main__":
    unittest.main()
