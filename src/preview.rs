use crate::command::{CommandResult, CommandStatus, exists, run};
use crate::discovery::dnf_binary;
use crate::model::{Impact, Match, Preview, PreviewStatus};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(90);

fn unknown(note: impl Into<String>) -> Preview {
    Preview {
        status: PreviewStatus::Unknown,
        impact: Impact::Unknown,
        planned_removals: Vec::new(),
        unused_dependencies: Vec::new(),
        protected: Vec::new(),
        blockers: Vec::new(),
        notes: vec![note.into()],
        fingerprint: String::new(),
    }
}

fn failed(result: &CommandResult, manager: &str) -> Preview {
    let combined = result.combined();
    let detail = combined
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no diagnostic output");
    let detail = detail.chars().take(240).collect::<String>();
    Preview {
        status: PreviewStatus::Failed,
        impact: Impact::Unknown,
        planned_removals: Vec::new(),
        unused_dependencies: Vec::new(),
        protected: Vec::new(),
        blockers: vec![format!("{manager} could not preview the removal: {detail}")],
        notes: Vec::new(),
        fingerprint: String::new(),
    }
}

fn blocked(note: impl Into<String>) -> Preview {
    Preview {
        status: PreviewStatus::Blocked,
        impact: Impact::Blocked,
        planned_removals: Vec::new(),
        unused_dependencies: Vec::new(),
        protected: Vec::new(),
        blockers: vec![note.into()],
        notes: Vec::new(),
        fingerprint: String::new(),
    }
}

fn lines_after(text: &str, headings: &[&str]) -> Vec<String> {
    let mut active = false;
    let mut found = Vec::new();
    for line in text.lines() {
        let clean = line.trim();
        let lower = clean.to_ascii_lowercase();
        if headings.iter().any(|heading| lower.starts_with(heading)) {
            active = true;
            continue;
        }
        if active && (clean.is_empty() || (!line.starts_with(' ') && clean.ends_with(':'))) {
            active = false;
            continue;
        }
        if active {
            let name = clean.split_whitespace().next().unwrap_or_default();
            if !name.is_empty()
                && !name.starts_with('=')
                && !name.chars().all(|character| character == '-')
            {
                found.push(name.to_owned());
            }
        }
    }
    found
}

fn dnf_preview(ids: &[String]) -> Preview {
    let Some(manager) = dnf_binary() else {
        return unknown("DNF is unavailable");
    };
    let mut args = vec!["--assumeno".to_owned(), "remove".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run(manager, &args, TIMEOUT);
    let text = result.combined();
    // DNF deliberately returns non-zero when --assumeno declines a valid plan.
    let requested: HashSet<String> = ids.iter().map(|id| crate::util::package_base(id)).collect();
    let all = lines_after(
        &text,
        &[
            "removing:",
            "removing dependent packages:",
            "removing unused dependencies:",
        ],
    );
    let unused = lines_after(&text, &["removing unused dependencies:"]);
    if all.is_empty() {
        return if result.status == CommandStatus::Timeout {
            unknown("DNF removal preview timed out")
        } else {
            failed(&result, manager)
        };
    }
    let mut preview = Preview::exact(all);
    preview.unused_dependencies = unused
        .into_iter()
        .filter(|name| !requested.contains(&crate::util::package_base(name)))
        .collect();
    preview
}

fn apt_preview(ids: &[String], purge: bool) -> Preview {
    let mut args = vec![
        "--simulate".to_owned(),
        if purge { "purge" } else { "remove" }.to_owned(),
    ];
    args.extend(ids.iter().cloned());
    let result = run("apt-get", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "APT");
    }
    let removals: Vec<String> = result
        .stdout
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Remv ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_owned)
        })
        .collect();
    if removals.is_empty() {
        return Preview {
            status: PreviewStatus::NoOp,
            impact: Impact::Low,
            planned_removals: Vec::new(),
            unused_dependencies: Vec::new(),
            protected: Vec::new(),
            blockers: Vec::new(),
            notes: vec!["APT reported that nothing would be removed".to_owned()],
            fingerprint: String::new(),
        };
    }
    Preview::exact(removals)
}

fn pacman_preview(ids: &[String]) -> Preview {
    let mut args = vec![
        "-R".to_owned(),
        "--print-format".to_owned(),
        "%n".to_owned(),
    ];
    args.extend(ids.iter().cloned());
    let result = run("pacman", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "Pacman");
    }
    let planned: Vec<String> = result
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if planned.is_empty() {
        failed(&result, "Pacman")
    } else {
        Preview::exact(planned)
    }
}

fn rpm_preview(ids: &[String]) -> Preview {
    let mut args = vec!["-e".to_owned(), "--test".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run("rpm", &args, TIMEOUT);
    if result.ok() {
        Preview::exact(ids.to_vec())
    } else {
        blocked(result.combined().trim().to_owned())
    }
}

fn rpm_ostree_preview(ids: &[String]) -> Preview {
    let mut args = vec!["uninstall".to_owned(), "--dry-run".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run("rpm-ostree", &args, TIMEOUT);
    if result.ok() {
        Preview::exact(ids.to_vec())
    } else {
        failed(&result, "rpm-ostree")
    }
}

fn zypper_preview(ids: &[String]) -> Preview {
    let mut args = vec![
        "--xmlout".to_owned(),
        "--non-interactive".to_owned(),
        "remove".to_owned(),
        "--dry-run".to_owned(),
    ];
    args.extend(ids.iter().cloned());
    let result = run("zypper", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "Zypper");
    }
    let expression = Regex::new(r#"(?i)<solvable\b[^>]*\bname=[\"']([^\"']+)[\"'][^>]*(?:\btransaction=[\"'](?:remove|erase)[\"']|\bstatus=[\"'](?:remove|erase)[\"'])"#).expect("valid expression");
    let mut planned: Vec<String> = expression
        .captures_iter(&result.stdout)
        .map(|found| found[1].to_owned())
        .collect();
    if planned.is_empty() {
        let alternate = Regex::new(r#"(?i)<solvable\b[^>]*(?:\btransaction=[\"'](?:remove|erase)[\"']|\bstatus=[\"'](?:remove|erase)[\"'])[^>]*\bname=[\"']([^\"']+)[\"']"#).expect("valid expression");
        planned = alternate
            .captures_iter(&result.stdout)
            .map(|found| found[1].to_owned())
            .collect();
    }
    if planned.is_empty() {
        let section = Regex::new(r"(?is)<to-remove>(.*?)</to-remove>").expect("valid expression");
        let names = Regex::new(r#"(?i)<solvable\b[^>]*\bname=[\"']([^\"']+)[\"']"#)
            .expect("valid expression");
        if let Some(removed) = section.captures(&result.stdout) {
            planned = names
                .captures_iter(&removed[1])
                .map(|found| found[1].to_owned())
                .collect();
        }
    }
    if planned.is_empty() {
        failed(&result, "Zypper")
    } else {
        Preview::exact(planned)
    }
}

fn apk_preview(ids: &[String]) -> Preview {
    let mut args = vec!["del".to_owned(), "--simulate".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run("apk", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "APK");
    }
    let expression =
        Regex::new(r"(?i)(?:purging|deleting|removing)\s+([^\s(]+)").expect("valid expression");
    let mut planned: Vec<String> = expression
        .captures_iter(&result.combined())
        .map(|found| found[1].trim_end_matches(':').to_owned())
        .collect();
    if planned.is_empty() {
        planned = result
            .stdout
            .lines()
            .filter_map(|line| {
                line.strip_prefix("- ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned)
            })
            .collect();
    }
    if planned.is_empty() {
        failed(&result, "APK")
    } else {
        let requested: HashSet<String> = ids.iter().cloned().collect();
        let mut preview = Preview::exact(planned.clone());
        preview.unused_dependencies = planned
            .into_iter()
            .filter(|name| !requested.contains(name))
            .collect();
        preview
    }
}

fn opkg_preview(ids: &[String]) -> Preview {
    let mut args = vec!["--noaction".to_owned(), "remove".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run("opkg", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "OPKG");
    }
    let expression =
        Regex::new(r"(?im)^(?:Removing|Not selecting)\s+([^\s.]+)").expect("valid expression");
    let planned: Vec<String> = expression
        .captures_iter(&result.combined())
        .map(|found| found[1].to_owned())
        .collect();
    if planned.is_empty() {
        unknown("OPKG did not provide a machine-readable removal plan")
    } else {
        Preview::exact(planned)
    }
}

fn xbps_preview(ids: &[String]) -> Preview {
    let mut args = vec!["--dry-run".to_owned(), "--recursive".to_owned()];
    args.extend(ids.iter().cloned());
    let result = run("xbps-remove", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "XBPS");
    }
    let version = Regex::new(r"^(.+)-[0-9][^\s]*_\d+$").expect("valid expression");
    let planned: Vec<String> = result
        .combined()
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(1) != Some(&"remove") {
                return None;
            }
            let package = *fields.first()?;
            version.captures(package).map(|found| found[1].to_owned())
        })
        .collect();
    if planned.is_empty() {
        unknown("XBPS did not provide a parseable removal plan")
    } else {
        let requested: HashSet<String> = ids.iter().cloned().collect();
        let mut preview = Preview::exact(planned.clone());
        preview.unused_dependencies = planned
            .into_iter()
            .filter(|name| !requested.contains(name))
            .collect();
        preview
    }
}

fn portage_preview(items: &[Match]) -> Preview {
    let mut args = vec![
        "--pretend".to_owned(),
        "--verbose".to_owned(),
        "--depclean".to_owned(),
    ];
    args.extend(items.iter().map(|item| {
        if item.version.is_empty() {
            format!("={}", item.id)
        } else {
            format!("={}-{}", item.id, item.version)
        }
    }));
    let result = run("emerge", &args, TIMEOUT);
    if !result.ok() {
        return failed(&result, "Portage");
    }
    let expression = Regex::new(r"(?m)^\s*>>>\s+([^\s]+/[^\s-]+)-[0-9]").expect("valid expression");
    let planned: Vec<String> = expression
        .captures_iter(&result.stdout)
        .map(|found| found[1].to_owned())
        .collect();
    if planned.is_empty() {
        unknown("Portage's pretend output did not contain a stable removal list")
    } else {
        let requested: HashSet<String> = items.iter().map(|item| item.id.clone()).collect();
        let mut preview = Preview::exact(planned.clone());
        preview.unused_dependencies = planned
            .into_iter()
            .filter(|name| !requested.contains(name))
            .collect();
        preview
    }
}

fn conda_preview(item: &Match) -> Preview {
    let manager = if item.backend == "Conda" {
        "conda"
    } else {
        "micromamba"
    };
    let result = run(
        manager,
        [
            "remove",
            "--dry-run",
            "--json",
            "--prefix",
            &item.profile,
            &item.id,
        ],
        TIMEOUT,
    );
    if !result.ok() {
        return failed(&result, manager);
    }
    if serde_json::from_str::<Value>(&result.stdout).is_ok() {
        Preview::exact(vec![item.id.clone()])
    } else {
        unknown(format!("{manager} did not return a valid JSON preview"))
    }
}

fn generic_preview(items: &[Match], manager_cleanup: bool) -> Preview {
    let backend = items[0].backend.as_str();
    let ids: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
    match backend {
        "DNF" => dnf_preview(&ids),
        "APT" => apt_preview(&ids, manager_cleanup),
        "APT-RPM" => {
            let mut args = vec!["--simulate".to_owned(), "remove".to_owned()];
            args.extend(ids.clone());
            let result = run("apt-get", &args, TIMEOUT);
            if result.ok() {
                Preview::exact(ids)
            } else {
                failed(&result, "APT-RPM")
            }
        }
        "Pacman" => pacman_preview(&ids),
        "RPM" => rpm_preview(&ids),
        "RPM-OSTree" => rpm_ostree_preview(&ids),
        "Zypper" => zypper_preview(&ids),
        "APK" => apk_preview(&ids),
        "OPKG" => opkg_preview(&ids),
        "XBPS" => xbps_preview(&ids),
        "Portage" => portage_preview(items),
        "Conda" | "Micromamba" => conda_preview(&items[0]),
        "Container Export" => blocked(
            "this is a host export of an application inside a container; remove it inside that container",
        ),
        "Standalone" => {
            unknown("no package manager can calculate dependencies for an unmanaged executable")
        }
        "Slackware" => unknown("Slackware pkgtools does not track dependency relationships"),
        "Eopkg" => {
            unknown("Eopkg's dry-run output is not stable machine-readable transaction data")
        }
        "Swupd" | "Swupd 3rd-party" => unknown("Swupd has no read-only bundle-removal transaction"),
        "URPMI" | "YUM" => unknown(format!(
            "{backend} does not expose a reliable machine-readable removal preview"
        )),
        "Snap" => unknown("snapd does not expose a read-only removal transaction"),
        "Homebrew" | "Homebrew Cask" => {
            let mut args = vec!["uninstall", "--dry-run"];
            if backend == "Homebrew Cask" {
                args.push("--cask");
            }
            let mut owned: Vec<String> = args.into_iter().map(str::to_owned).collect();
            owned.extend(ids.clone());
            let result = run("brew", &owned, TIMEOUT);
            if result.ok() {
                Preview::exact(ids)
            } else {
                unknown("Homebrew could not produce a reliable dry-run")
            }
        }
        "Flatpak" | "Gear Lever" | "AppImage" | "Cargo" | "Pipx" | "UV Tool" | "NPM" | "Nix"
        | "Nix Legacy" | "Guix" => Preview::exact(ids),
        _ => unknown(format!("{backend} has no supported removal preview")),
    }
}

fn protected_names() -> HashSet<String> {
    let mut names: HashSet<String> = [
        "apt",
        "base",
        "base-files",
        "bash",
        "busybox",
        "coreutils",
        "dbus",
        "dnf",
        "dnf5",
        "filesystem",
        "glibc",
        "grub",
        "kernel",
        "linux",
        "musl",
        "openrc",
        "pacman",
        "rpm",
        "rpm-ostree",
        "shadow",
        "snapd",
        "sudo",
        "systemd",
        "transactional-update",
        "util-linux",
        "zypper",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if let Ok(executable) = std::fs::read_link("/proc/1/exe") {
        if let Some(name) = executable.file_name().and_then(|name| name.to_str()) {
            names.insert(name.to_owned());
        }
    }
    if exists("uname") {
        let release = crate::command::output("uname", &["-r"]);
        if !release.trim().is_empty() {
            names.insert(format!("kernel-{}", release.trim()));
            names.insert(format!("linux-image-{}", release.trim()));
        }
    }
    if exists("apt-mark") {
        names.extend(
            crate::command::output("apt-mark", &["showhold"])
                .lines()
                .map(crate::util::package_base),
        );
    }
    for directory in ["/etc/dnf/protected.d", "/etc/yum/protected.d"] {
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                if let Ok(text) = std::fs::read_to_string(entry.path()) {
                    names.extend(
                        text.lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty() && !line.starts_with('#'))
                            .filter(|line| !line.contains(['*', '?', '[']))
                            .map(str::to_owned),
                    );
                }
            }
        }
    }
    names
}

fn is_protected(name: &str, protected: &HashSet<String>) -> bool {
    let base = crate::util::package_base(name).to_ascii_lowercase();
    protected.contains(&base)
        || base == "kernel-core"
        || base.starts_with("kernel-modules")
        || base.starts_with("kernel-core-")
        || base.starts_with("linux-image")
}

fn fingerprint(preview: &Preview, items: &[Match], cleanup_backends: &[String]) -> String {
    let value = json!({
        "items": items.iter().map(|item| (&item.backend, &item.id, &item.version, &item.scope, &item.installation, &item.profile)).collect::<Vec<_>>(),
        "manager_cleanup": cleanup_backends,
        "status": preview.status,
        "impact": preview.impact,
        "planned": preview.planned_removals,
        "unused": preview.unused_dependencies,
        "protected": preview.protected,
        "blockers": preview.blockers,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&value).expect("serializable preview"),
    ))
}

fn build_group(items: &[Match], manager_cleanup: bool) -> Preview {
    if items.is_empty() {
        return blocked("no application was selected");
    }
    let mut preview = generic_preview(items, manager_cleanup);
    let requested: HashSet<String> = items
        .iter()
        .map(|item| crate::util::package_base(&item.id))
        .collect();
    let protected = protected_names();
    preview.protected = preview
        .planned_removals
        .iter()
        .filter(|name| is_protected(name, &protected))
        .cloned()
        .collect();
    preview.unused_dependencies.sort();
    preview.unused_dependencies.dedup();
    let unused: HashSet<String> = preview
        .unused_dependencies
        .iter()
        .map(|name| crate::util::package_base(name))
        .collect();
    let extra: BTreeSet<String> = preview
        .planned_removals
        .iter()
        .filter(|name| {
            let base = crate::util::package_base(name);
            !requested.contains(&base) && !unused.contains(&base)
        })
        .cloned()
        .collect();
    if preview.status == PreviewStatus::Exact {
        preview.impact = if !preview.protected.is_empty() || !extra.is_empty() {
            Impact::High
        } else if !preview.unused_dependencies.is_empty() {
            Impact::Caution
        } else {
            Impact::Low
        };
    }
    if !preview.protected.is_empty() {
        preview.impact = Impact::High;
    }
    if matches!(
        preview.status,
        PreviewStatus::Unknown | PreviewStatus::Failed | PreviewStatus::Unsupported
    ) {
        preview.impact = Impact::Unknown;
    }
    if preview.status == PreviewStatus::Blocked {
        preview.impact = Impact::Blocked;
    }
    preview
}

fn status_rank(status: PreviewStatus) -> u8 {
    match status {
        PreviewStatus::Blocked => 5,
        PreviewStatus::Failed => 4,
        PreviewStatus::Unknown | PreviewStatus::Unsupported => 3,
        PreviewStatus::Exact => 2,
        PreviewStatus::NoOp => 1,
    }
}

fn impact_rank(impact: Impact) -> u8 {
    match impact {
        Impact::Blocked => 5,
        Impact::Unknown => 4,
        Impact::High => 3,
        Impact::Caution => 2,
        Impact::Low => 1,
    }
}

pub fn build(items: &[Match], cleanup_backends: &[String]) -> Preview {
    if items.is_empty() {
        return blocked("no application was selected");
    }
    let mut groups: std::collections::BTreeMap<String, Vec<Match>> =
        std::collections::BTreeMap::new();
    for item in items {
        groups
            .entry(format!(
                "{}\0{}\0{}\0{}",
                item.backend, item.scope, item.installation, item.profile
            ))
            .or_default()
            .push(item.clone());
    }
    let mut previews: Vec<Preview> = groups
        .into_values()
        .map(|group| {
            let cleanup = cleanup_backends
                .iter()
                .any(|backend| backend == &group[0].backend);
            build_group(&group, cleanup)
        })
        .collect();
    let mut preview = previews
        .pop()
        .unwrap_or_else(|| blocked("no application was selected"));
    for next in previews {
        if status_rank(next.status) > status_rank(preview.status) {
            preview.status = next.status;
        }
        if impact_rank(next.impact) > impact_rank(preview.impact) {
            preview.impact = next.impact;
        }
        preview.planned_removals.extend(next.planned_removals);
        preview.unused_dependencies.extend(next.unused_dependencies);
        preview.protected.extend(next.protected);
        preview.blockers.extend(next.blockers);
        preview.notes.extend(next.notes);
    }
    for values in [
        &mut preview.planned_removals,
        &mut preview.unused_dependencies,
        &mut preview.protected,
        &mut preview.blockers,
        &mut preview.notes,
    ] {
        values.sort();
        values.dedup();
    }
    preview.fingerprint = fingerprint(&preview, items, cleanup_backends);
    preview
}

pub fn ensure_manager_present(items: &[Match]) -> Result<(), String> {
    for item in items {
        let manager = match item.backend.as_str() {
            "APT" | "APT-RPM" => "apt-get",
            "DNF" => dnf_binary().unwrap_or("dnf"),
            "YUM" => "yum",
            "RPM" => "rpm",
            "RPM-OSTree" => "rpm-ostree",
            "Zypper" => "zypper",
            "URPMI" => "urpme",
            "Pacman" => "pacman",
            "APK" => "apk",
            "OPKG" => "opkg",
            "XBPS" => "xbps-remove",
            "Portage" => "emerge",
            "Slackware" => "removepkg",
            "Eopkg" => "eopkg",
            "Swupd" | "Swupd 3rd-party" => "swupd",
            "Flatpak" => "flatpak",
            "Snap" => "snap",
            "Homebrew" | "Homebrew Cask" => "brew",
            "Cargo" => "cargo",
            "Pipx" => "pipx",
            "UV Tool" => "uv",
            "NPM" => "npm",
            "Nix" => "nix",
            "Nix Legacy" => "nix-env",
            "Guix" => "guix",
            "Conda" => "conda",
            "Micromamba" => "micromamba",
            "AppImage" | "Standalone" => "rm",
            "Gear Lever" => continue,
            "Container Export" => {
                return Err("container exports cannot be removed from the host".to_owned());
            }
            other => return Err(format!("unsupported backend: {other}")),
        };
        if !exists(manager) {
            return Err(format!("{manager} is no longer available"));
        }
    }
    Ok(())
}

pub fn transactional_system() -> bool {
    Path::new("/run/transactional-update.conf").exists() && exists("transactional-update")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_single_target_is_low_impact() {
        let preview = Preview::exact(vec!["example".to_owned()]);
        assert_eq!(preview.impact, Impact::Low);
    }

    #[test]
    fn fingerprint_changes_with_cleanup() {
        let item = Match::new("Flatpak", "org.example.App", "Example");
        let preview = Preview::exact(vec![item.id.clone()]);
        assert_ne!(
            fingerprint(&preview, std::slice::from_ref(&item), &[]),
            fingerprint(&preview, &[item], &["Flatpak".to_owned()])
        );
    }
}
