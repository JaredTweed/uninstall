use clap::{CommandFactory, Parser};
use serde_json::json;
use std::collections::{BTreeSet, HashSet};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use uninstall::cleanup::{self, CleanupCandidate, DataOption};
use uninstall::command::{output, run as command_run};
use uninstall::discovery;
use uninstall::model::{Impact, JsonResult, Match, Preview, PreviewStatus};
use uninstall::preview;
use uninstall::provenance;
use uninstall::removal;
use uninstall::util::{command_string, format_size, norm, package_base, parse_size, sanitize};

#[derive(Debug, Parser)]
#[command(
    name = "uninstall",
    version,
    about = "Find, explain, and safely remove installed Linux software.",
    after_help = "Examples:\n  uninstall FreeCAD          find and remove it\n  uninstall DOSbox           explain and remove it\n  uninstall ./example.rpm    resolve a local archive"
)]
struct Cli {
    /// App name, command, package ID, or local package archive
    app: Option<String>,

    /// Remove the uninstall command itself
    #[arg(long)]
    self_uninstall: bool,

    /// Include fuzzy library/dependency search matches
    #[arg(long)]
    show_dependencies: bool,

    /// Print a read-only machine-readable report
    #[arg(long)]
    json: bool,

    /// Include backend timing and failure diagnostics
    #[arg(long)]
    debug: bool,

    /// Exact backend for guarded non-interactive removal
    #[arg(long)]
    backend: Option<String>,

    /// Exact `REMOVE Backend:package-id` authorization
    #[arg(long)]
    confirm: Option<String>,
}

fn prompt(message: &str) -> io::Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

fn yes(message: &str) -> bool {
    prompt(message).is_ok_and(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn progress<F, T>(message: &'static str, function: F) -> T
where
    F: FnOnce() -> T,
{
    let (finished, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_millis(600)).is_err() {
            eprintln!("{message}");
        }
    });
    let result = function();
    let _ = finished.send(());
    let _ = handle.join();
    result
}

fn detail_identifier(item: &Match) -> Option<String> {
    let package_id = package_base(item.id.split(':').next().unwrap_or(&item.id));
    if matches!(
        item.backend.as_str(),
        "Flatpak" | "Gear Lever" | "AppImage" | "Standalone" | "Container Export"
    ) || norm(&package_id) != norm(&item.name)
    {
        Some(item.id.clone())
    } else {
        None
    }
}

fn show_matches(items: &[Match]) {
    println!(
        "\nFound {} likely installed option{}:\n",
        items.len(),
        if items.len() == 1 { "" } else { "s" }
    );
    for (index, item) in items.iter().enumerate() {
        let mut attributes = vec![item.scope.clone()];
        if item.role != uninstall::model::Role::Explicit {
            attributes.push(item.role.label().to_owned());
        }
        if let Some(size) = item.installed_size_bytes {
            attributes.push(format_size(size));
        }
        let version = if item.version.is_empty() {
            String::new()
        } else {
            format!("  {}", sanitize(&item.version))
        };
        println!(
            "{:>4}. {}  [{}]{} | {}",
            index + 1,
            sanitize(&item.name),
            sanitize(&item.backend),
            version,
            attributes
                .into_iter()
                .map(|value| sanitize(&value))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        if let Some(identifier) = detail_identifier(item) {
            println!("      {}", sanitize(&identifier));
        }
        if let Some(path) = &item.command_path {
            println!(
                "      provides command: {}",
                sanitize(&path.display().to_string())
            );
        }
        println!("      Why installed: {}", sanitize(&item.reason));
        if item.backend == "Standalone" {
            println!("      note: related files cannot be identified automatically");
        }
        if !item.evidence.is_empty() {
            println!("      archive evidence: {}", sanitize(&item.evidence));
        }
    }
}

fn parse_choices(answer: &str, count: usize) -> Option<Vec<usize>> {
    if answer.trim().eq_ignore_ascii_case("a") {
        return Some((0..count).collect());
    }
    let mut choices = BTreeSet::new();
    for part in answer
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let number = part.parse::<usize>().ok()?;
        if number == 0 || number > count {
            return None;
        }
        choices.insert(number - 1);
    }
    Some(choices.into_iter().collect())
}

fn choose(items: &[Match]) -> Vec<Match> {
    if items.len() == 1 {
        println!("\nAutomatically selected the only result.");
        return items.to_vec();
    }
    println!("\nChoose numbers separated by commas, 'a' for all, or Enter to cancel.");
    loop {
        let Ok(answer) = prompt("> ") else {
            return Vec::new();
        };
        if answer.is_empty() {
            return Vec::new();
        }
        if let Some(indexes) = parse_choices(&answer, items.len()) {
            return indexes
                .into_iter()
                .map(|index| items[index].clone())
                .collect();
        }
        println!(
            "Enter valid numbers from 1 to {}, 'a', or press Enter to cancel.",
            items.len()
        );
    }
}

fn requested_names(items: &[Match]) -> HashSet<String> {
    items.iter().map(|item| package_base(&item.id)).collect()
}

fn compact_names(names: &[String]) -> String {
    const LIMIT: usize = 12;
    if names.len() <= LIMIT {
        names
            .iter()
            .map(|name| sanitize(name))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!(
            "{} and {} more",
            names[..LIMIT]
                .iter()
                .map(|name| sanitize(name))
                .collect::<Vec<_>>()
                .join(", "),
            names.len() - LIMIT
        )
    }
}

fn show_impact(plan: &Preview, selected: &[Match]) {
    let requested = requested_names(selected);
    let unused: HashSet<&String> = plan.unused_dependencies.iter().collect();
    let additional: Vec<String> = plan
        .planned_removals
        .iter()
        .filter(|name| !requested.contains(&package_base(name)) && !unused.contains(name))
        .cloned()
        .collect();
    if !additional.is_empty() {
        println!(
            "\nAlso expected to remove {} other package{}: {}",
            additional.len(),
            if additional.len() == 1 { "" } else { "s" },
            compact_names(&additional)
        );
    }
    if !plan.unused_dependencies.is_empty() {
        println!(
            "\nAlso expected to remove {} now-unused dependenc{}: {}",
            plan.unused_dependencies.len(),
            if plan.unused_dependencies.len() == 1 {
                "y"
            } else {
                "ies"
            },
            compact_names(&plan.unused_dependencies)
        );
    }
    if !plan.protected.is_empty() {
        println!(
            "\nWarning: the plan includes protected or critical packages: {}",
            compact_names(&plan.protected)
        );
    }
    for blocker in &plan.blockers {
        println!("\nCannot safely continue: {}", sanitize(blocker));
    }
    if matches!(
        plan.status,
        PreviewStatus::Unknown | PreviewStatus::Failed | PreviewStatus::Unsupported
    ) {
        println!(
            "\nWarning: a reliable preview was unavailable; review the package manager's final transaction carefully."
        );
    }
}

fn show_data_options(options: &[DataOption]) {
    println!("\nRemove associated data too? (optional)\n");
    let width = options
        .iter()
        .map(|option| option.label.chars().count())
        .max()
        .unwrap_or(0)
        + 3;
    for (index, option) in options.iter().enumerate() {
        let size = option
            .size_bytes
            .map(format_size)
            .unwrap_or_else(|| "size unknown".to_owned());
        println!(
            "{:>4}. {:width$} {}",
            index + 1,
            sanitize(&option.label),
            size,
            width = width
        );
    }
    let managers: BTreeSet<&str> = options
        .iter()
        .filter_map(|option| option.backend.as_deref())
        .collect();
    let manager_note = if managers.len() == 1 {
        format!(
            "{} data is manager-owned.",
            managers.first().copied().unwrap_or("Package-manager")
        )
    } else if managers.is_empty() {
        String::new()
    } else {
        "Package-manager data is manager-owned.".to_owned()
    };
    let separator = if manager_note.is_empty() { "" } else { " " };
    println!(
        "\n{manager_note}{separator}Detected paths are exact name matches, are not guaranteed to belong to the app, and will be deleted permanently."
    );
    println!("Choose numbers separated by commas, 'a' for all, or Enter to keep everything.");
}

fn choose_data(options: &[DataOption]) -> Vec<usize> {
    if options.is_empty() {
        return Vec::new();
    }
    show_data_options(options);
    loop {
        let Ok(answer) = prompt("> ") else {
            return Vec::new();
        };
        if answer.is_empty() {
            return Vec::new();
        }
        if let Some(choices) = parse_choices(&answer, options.len()) {
            return choices;
        }
        println!(
            "Enter valid numbers from 1 to {}, 'a', or press Enter to keep everything.",
            options.len()
        );
    }
}

fn installed_version(item: &Match) -> Option<String> {
    let id = item.id.as_str();
    let text = match item.backend.as_str() {
        "APT" => output(
            "dpkg-query",
            &["-W", "-f=${db:Status-Abbrev}\t${Version}\n", id],
        ),
        "DNF" | "YUM" | "RPM" | "RPM-OSTree" | "Zypper" | "URPMI" | "APT-RPM" => {
            output("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}\n", "--", id])
        }
        "Pacman" => output("pacman", &["-Q", id])
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned(),
        "APK" => output("apk", &["info", "--all", id])
            .lines()
            .find_map(|line| {
                line.strip_prefix(&format!("{id}-"))
                    .and_then(|suffix| suffix.split_whitespace().next())
            })
            .unwrap_or_default()
            .to_owned(),
        "XBPS" => {
            let raw = output("xbps-query", &["--property", "pkgver", id]);
            let value = raw
                .trim()
                .strip_prefix("pkgver:")
                .unwrap_or(raw.trim())
                .trim();
            value
                .strip_prefix(&format!("{id}-"))
                .unwrap_or(value)
                .to_owned()
        }
        "Flatpak" => {
            let location = if item.scope == "user" {
                "--user".to_owned()
            } else if item.installation.is_empty() {
                "--system".to_owned()
            } else {
                format!("--installation={}", item.installation)
            };
            output("flatpak", &["info", &location, "--show-version", id])
        }
        "Snap" => output("snap", &["list", id])
            .lines()
            .nth(1)
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_owned(),
        "Homebrew" => output("brew", &["list", "--versions", id])
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned(),
        "Homebrew Cask" => output("brew", &["list", "--cask", "--versions", id])
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned(),
        "Cargo" => output("cargo", &["install", "--list"])
            .lines()
            .find(|line| line.starts_with(&format!("{id} v")))
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .trim_start_matches('v')
            .trim_end_matches(':')
            .to_owned(),
        "AppImage" | "Standalone" | "Gear Lever" => {
            return item
                .source_path
                .as_ref()
                .filter(|path| path.exists() || path.is_symlink())
                .map(|_| item.version.clone());
        }
        _ => return None,
    };
    let value = if item.backend == "APT" {
        text.split_once('\t')
            .filter(|(status, _)| status.as_bytes().get(1) == Some(&b'i'))
            .map(|(_, version)| version.trim())
            .unwrap_or("")
    } else {
        text.trim()
    };
    (!value.is_empty()).then(|| value.lines().next().unwrap_or_default().to_owned())
}

fn revalidate(items: &[Match]) -> Result<(), String> {
    preview::ensure_manager_present(items)?;
    for item in items {
        if present(item) == Some(false) {
            return Err(format!("{} is no longer installed", item.name));
        }
        if !item.version.is_empty() {
            if let Some(current) = installed_version(item) {
                if current != item.version {
                    return Err(format!(
                        "{} changed from version {} to {} after selection",
                        item.name, item.version, current
                    ));
                }
            }
        }
    }
    Ok(())
}

fn present(item: &Match) -> Option<bool> {
    let checked = |program: &str, args: &[&str]| {
        let result = command_run(program, args, Duration::from_secs(30));
        result.completed().then_some(result.ok())
    };
    match item.backend.as_str() {
        "AppImage" | "Standalone" => item
            .source_path
            .as_ref()
            .map(|path| path.exists() || path.is_symlink()),
        "Gear Lever" => item
            .source_path
            .as_ref()
            .map(|path| path.exists() || path.is_symlink()),
        "Flatpak" => Some(installed_version(item).is_some()),
        "APT" => Some(installed_version(item).is_some()),
        "DNF" | "YUM" | "RPM" | "RPM-OSTree" | "Zypper" | "URPMI" | "APT-RPM" => {
            Some(installed_version(item).is_some())
        }
        "Pacman" | "APK" | "XBPS" | "Snap" | "Homebrew" | "Homebrew Cask" | "Cargo" => {
            Some(installed_version(item).is_some())
        }
        "OPKG" => {
            let result = command_run("opkg", ["status", &item.id], Duration::from_secs(30));
            result.completed().then(|| {
                result.ok()
                    && result
                        .stdout
                        .to_ascii_lowercase()
                        .contains("status: install")
                    && result.stdout.to_ascii_lowercase().contains("installed")
            })
        }
        "Portage" => {
            let prefix =
                std::env::var_os("EPREFIX").map_or_else(|| PathBuf::from("/"), PathBuf::from);
            let (category, name) = item.id.split_once('/')?;
            std::fs::read_dir(prefix.join("var/db/pkg").join(category))
                .ok()
                .map(|entries| {
                    entries.flatten().any(|entry| {
                        let package = entry.file_name().to_string_lossy().into_owned();
                        package == name
                            || package
                                .strip_prefix(&format!("{name}-"))
                                .is_some_and(|suffix| suffix.starts_with(char::is_numeric))
                    })
                })
        }
        "Slackware" => std::fs::read_dir("/var/log/packages").ok().map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .strip_prefix(&format!("{}-", item.id))
                    .is_some_and(|suffix| suffix.starts_with(char::is_numeric))
            })
        }),
        "Eopkg" => checked("eopkg", &["info", "--installed", &item.id]),
        "Swupd" => {
            let result = command_run(
                "swupd",
                ["bundle-list", "--status", "--quiet"],
                Duration::from_secs(30),
            );
            result.completed().then(|| {
                result.ok()
                    && result.stdout.lines().any(|line| {
                        line.split_once(':').is_some_and(|(name, status)| {
                            name.trim() == item.id
                                && status.to_ascii_lowercase().contains("installed")
                        })
                    })
            })
        }
        "Swupd 3rd-party" => {
            let result = command_run(
                "swupd",
                ["3rd-party", "bundle-list", "--repo", &item.origin],
                Duration::from_secs(30),
            );
            result.completed().then(|| {
                result.ok()
                    && result
                        .stdout
                        .lines()
                        .any(|line| line.split_whitespace().next() == Some(item.id.as_str()))
            })
        }
        "Pipx" => {
            let args = if item.scope == "system" {
                vec!["list", "--json", "--global"]
            } else {
                vec!["list", "--json"]
            };
            let result = command_run("pipx", args, Duration::from_secs(30));
            result.completed().then(|| {
                result.ok()
                    && serde_json::from_str::<serde_json::Value>(&result.stdout)
                        .ok()
                        .and_then(|value| value.get("venvs").cloned())
                        .and_then(|value| value.as_object().cloned())
                        .is_some_and(|venvs| venvs.contains_key(&item.id))
            })
        }
        "UV Tool" => {
            let result = command_run("uv", ["tool", "list"], Duration::from_secs(30));
            result.completed().then(|| {
                result.ok()
                    && result.stdout.lines().any(|line| {
                        !line.starts_with(char::is_whitespace)
                            && line.split_whitespace().next() == Some(item.id.as_str())
                    })
            })
        }
        "NPM" => {
            let result = command_run(
                "npm",
                ["list", "--global", "--depth=0", "--json"],
                Duration::from_secs(30),
            );
            result.completed().then(|| {
                result.ok()
                    && serde_json::from_str::<serde_json::Value>(&result.stdout)
                        .ok()
                        .and_then(|value| value.get("dependencies").cloned())
                        .and_then(|value| value.as_object().cloned())
                        .is_some_and(|packages| packages.contains_key(&item.id))
            })
        }
        "Nix" => {
            let mut args = vec!["profile", "list", "--json"];
            if !item.profile.is_empty() {
                args.extend(["--profile", &item.profile]);
            }
            let result = command_run("nix", args, Duration::from_secs(30));
            result.completed().then(|| {
                result.ok()
                    && serde_json::from_str::<serde_json::Value>(&result.stdout)
                        .ok()
                        .map(|value| value.get("elements").cloned().unwrap_or(value))
                        .and_then(|value| value.as_object().cloned())
                        .is_some_and(|elements| elements.contains_key(&item.id))
            })
        }
        "Nix Legacy" => {
            let result = command_run(
                "nix-env",
                ["--query", "--installed"],
                Duration::from_secs(30),
            );
            result.completed().then(|| {
                result.ok()
                    && result.stdout.lines().any(|line| {
                        line == item.id
                            || line
                                .strip_prefix(&format!("{}-", item.id))
                                .is_some_and(|suffix| suffix.starts_with(char::is_numeric))
                    })
            })
        }
        "Guix" => {
            let profile = format!("--profile={}", item.profile);
            checked("guix", &["package", &profile, "--list-installed", &item.id])
        }
        "Conda" | "Micromamba" => {
            let program = if item.backend == "Conda" {
                "conda"
            } else {
                "micromamba"
            };
            let result = command_run(
                program,
                ["list", "--json", "--prefix", &item.profile, &item.id],
                Duration::from_secs(30),
            );
            result.completed().then(|| {
                result.ok()
                    && serde_json::from_str::<serde_json::Value>(&result.stdout)
                        .ok()
                        .and_then(|value| value.as_array().cloned())
                        .is_some_and(|packages| {
                            packages.iter().any(|package| {
                                package.get("name").and_then(serde_json::Value::as_str)
                                    == Some(item.id.as_str())
                            })
                        })
            })
        }
        _ => None,
    }
}

fn estimate(
    selected: &[Match],
    plan: &Preview,
    options: &[DataOption],
    choices: &[usize],
) -> (u64, bool) {
    let requested = requested_names(selected);
    let selected_known = selected
        .iter()
        .filter_map(|item| item.installed_size_bytes)
        .sum::<u64>();
    let selected_complete = selected
        .iter()
        .all(|item| item.installed_size_bytes.is_some());
    let data_known = choices
        .iter()
        .filter_map(|index| options[*index].size_bytes)
        .sum::<u64>();
    let data_complete = choices
        .iter()
        .all(|index| options[*index].size_bytes.is_some());
    let extra: Vec<String> = plan
        .planned_removals
        .iter()
        .filter(|name| !requested.contains(&package_base(name)))
        .cloned()
        .collect();
    let (extra_known, extra_complete) = planned_package_sizes(selected, &extra);
    (
        selected_known
            .saturating_add(extra_known)
            .saturating_add(data_known),
        selected_complete && extra_complete && data_complete,
    )
}

fn planned_package_sizes(selected: &[Match], names: &[String]) -> (u64, bool) {
    if names.is_empty() {
        return (0, true);
    }
    let backends: BTreeSet<&str> = selected.iter().map(|item| item.backend.as_str()).collect();
    let Some(backend) = backends
        .iter()
        .copied()
        .next()
        .filter(|_| backends.len() == 1)
    else {
        return (0, false);
    };
    let mut sizes = std::collections::HashMap::new();
    match backend {
        "DNF" | "YUM" | "RPM" | "RPM-OSTree" | "Zypper" | "URPMI" | "APT-RPM" => {
            let mut args = vec![
                "-q".to_owned(),
                "--qf".to_owned(),
                "%{NAME}\t%{SIZE}\n".to_owned(),
                "--".to_owned(),
            ];
            args.extend(names.iter().cloned());
            let result = command_run("rpm", args, Duration::from_secs(30));
            if result.ok() {
                for line in result.stdout.lines() {
                    if let Some((name, size)) = line.split_once('\t') {
                        if let Ok(size) = size.trim().parse::<u64>() {
                            sizes.insert(package_base(name), size);
                        }
                    }
                }
            }
        }
        "APT" => {
            let mut args = vec![
                "-W".to_owned(),
                "-f=${binary:Package}\t${Installed-Size}\n".to_owned(),
            ];
            args.extend(names.iter().cloned());
            let result = command_run("dpkg-query", args, Duration::from_secs(30));
            if result.ok() {
                for line in result.stdout.lines() {
                    if let Some((name, size)) = line.split_once('\t') {
                        if let Ok(size) = size.trim().parse::<u64>() {
                            sizes.insert(package_base(name), size.saturating_mul(1024));
                        }
                    }
                }
            }
        }
        "Pacman" => {
            let mut args = vec!["-Qi".to_owned()];
            args.extend(names.iter().cloned());
            let result = command_run("pacman", args, Duration::from_secs(30));
            if result.ok() {
                for record in result.stdout.split("\n\n") {
                    let name = record.lines().find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.trim() == "Name")
                            .map(|(_, value)| value.trim())
                    });
                    let size = record.lines().find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.trim() == "Installed Size")
                            .and_then(|(_, value)| parse_size(value.trim()))
                    });
                    if let (Some(name), Some(size)) = (name, size) {
                        sizes.insert(package_base(name), size);
                    }
                }
            }
        }
        _ => return (0, false),
    }
    let total = names
        .iter()
        .filter_map(|name| sizes.get(&package_base(name)))
        .sum();
    (
        total,
        names
            .iter()
            .all(|name| sizes.contains_key(&package_base(name))),
    )
}

fn ready_heading(size: u64, complete: bool) -> String {
    if size == 0 {
        "Ready to run:".to_owned()
    } else if complete {
        format!("Ready to run (freeing about {}):", format_size(size))
    } else {
        format!(
            "Ready to run (freeing at least {}; some sizes unknown):",
            format_size(size)
        )
    }
}

fn file_targets(selected: &[Match]) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for item in selected
        .iter()
        .filter(|item| matches!(item.backend.as_str(), "AppImage" | "Standalone"))
    {
        if let Some(path) = &item.source_path {
            paths.insert(path.clone());
        }
        if item.backend == "AppImage" {
            if let (Some(path), Some(exposed)) = (&item.source_path, &item.command_path) {
                if path != exposed
                    && std::fs::canonicalize(path).ok() == std::fs::canonicalize(exposed).ok()
                {
                    paths.insert(exposed.clone());
                }
            }
        }
    }
    paths.into_iter().collect()
}

fn run_command(command: &[String]) -> io::Result<i32> {
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]).env("LC_ALL", "C");
    child.status().map(|status| status.code().unwrap_or(1))
}

fn execute(
    selected: &[Match],
    plan: &Preview,
    cleanup_backends: &[String],
    data_candidates: Vec<CleanupCandidate>,
    file_candidates: Vec<CleanupCandidate>,
) -> i32 {
    if let Err(error) = revalidate(selected) {
        eprintln!("Cannot continue: {}. Nothing was run.", sanitize(&error));
        return 1;
    }
    let repeated = preview::build(selected, cleanup_backends);
    if repeated.fingerprint != plan.fingerprint {
        eprintln!(
            "The package-manager transaction changed after confirmation. Nothing was run; start the command again."
        );
        return 1;
    }
    let batches = match removal::build_batches(selected, cleanup_backends) {
        Ok(batches) => batches,
        Err(error) => {
            eprintln!("{}.", sanitize(&error));
            return 1;
        }
    };
    let commands = match prepare_commands(
        selected,
        cleanup_backends,
        plan.status == PreviewStatus::Exact,
    ) {
        Ok(commands) => commands,
        Err(error) => {
            eprintln!("{}.", sanitize(&error));
            return 1;
        }
    };
    let direct_files: HashSet<String> = selected
        .iter()
        .filter(|item| matches!(item.backend.as_str(), "AppImage" | "Standalone"))
        .map(Match::key)
        .collect();
    let mut failed = false;
    let mut file_candidates = file_candidates;
    for (batch, command) in batches.into_iter().zip(commands) {
        let names = batch
            .items
            .iter()
            .map(|item| sanitize(&item.name))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "\nRemoving {names} [{}]...",
            sanitize(&batch.items[0].backend)
        );
        if batch
            .items
            .iter()
            .all(|item| direct_files.contains(&item.key()))
            && !["sudo", "doas", "pkexec"].iter().any(|helper| {
                Path::new(&command[0])
                    .file_name()
                    .is_some_and(|name| name == *helper)
            })
        {
            let targets: HashSet<PathBuf> = file_targets(&batch.items).into_iter().collect();
            let mut retained = Vec::new();
            for candidate in std::mem::take(&mut file_candidates) {
                if !targets.contains(&candidate.path) {
                    retained.push(candidate);
                } else if let Err(error) = cleanup::remove(candidate) {
                    eprintln!("{}", sanitize(&error));
                    failed = true;
                }
            }
            file_candidates = retained;
        } else {
            match run_command(&command) {
                Ok(0) => {}
                Ok(code) => {
                    eprintln!("Removal failed with exit code {code}.");
                    failed = true;
                }
                Err(error) => {
                    eprintln!("Could not run {}: {error}", sanitize(&command[0]));
                    failed = true;
                }
            }
        }
    }
    if !failed {
        for candidate in data_candidates {
            let display = candidate.path.display().to_string();
            match cleanup::remove(candidate) {
                Ok(()) => println!("  Removed {}", sanitize(&display)),
                Err(error) => {
                    eprintln!(
                        "  Could not remove {}: {}",
                        sanitize(&display),
                        sanitize(&error)
                    );
                    failed = true;
                }
            }
        }
    } else if !data_candidates.is_empty() {
        println!("\nKept associated data because an application removal failed.");
    }
    println!("\nResult:");
    for item in selected {
        match present(item) {
            Some(false) => println!(
                "  Removed: {} [{}]",
                sanitize(&item.name),
                sanitize(&item.backend)
            ),
            Some(true) => {
                println!(
                    "  Still installed: {} [{}]",
                    sanitize(&item.name),
                    sanitize(&item.backend)
                );
                failed = true;
            }
            None => println!(
                "  Pending or unverifiable: {} [{}]",
                sanitize(&item.name),
                sanitize(&item.backend)
            ),
        }
    }
    println!(
        "\n{}",
        if failed {
            "Finished with errors."
        } else {
            "Finished."
        }
    );
    i32::from(failed)
}

fn prepare_commands(
    selected: &[Match],
    cleanup_backends: &[String],
    exact: bool,
) -> Result<Vec<Vec<String>>, String> {
    removal::build_batches(selected, cleanup_backends)?
        .into_iter()
        .map(|batch| {
            let command = removal::pin_command(&batch.command)?;
            Ok(if exact {
                removal::make_noninteractive(command, &batch.items[0].backend)
            } else {
                command
            })
        })
        .collect()
}

fn json_report(query: &str, show_dependencies: bool) -> i32 {
    let mut matches = discovery::find_matches(query);
    provenance::decorate(&mut matches);
    let (matches, hidden) = discovery::filter_dependencies(matches, query, show_dependencies);
    let results: Vec<JsonResult> = matches
        .iter()
        .map(|item| JsonResult {
            backend: item.backend.clone(),
            id: item.id.clone(),
            name: item.name.clone(),
            version: item.version.clone(),
            architecture: item.architecture.clone(),
            scope: item.scope.clone(),
            profile: item.profile.clone(),
            installation: item.installation.clone(),
            command_path: item
                .command_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            role: item.role,
            why_installed: item.reason.clone(),
            installed_size_bytes: item.installed_size_bytes,
            preview: preview::build(std::slice::from_ref(item), &[]),
        })
        .collect();
    println!("{}", serde_json::to_string(&json!({"schema_version": 1, "query": query, "hidden_dependency_matches": hidden, "results": results, "diagnostics": []})).expect("JSON report"));
    if matches.is_empty() { 1 } else { 0 }
}

fn self_uninstall() -> i32 {
    let invoked = std::env::args_os()
        .next()
        .map(PathBuf::from)
        .unwrap_or_default();
    let target = if invoked.components().count() == 1 {
        uninstall::command::which(&invoked.to_string_lossy()).unwrap_or(invoked)
    } else {
        invoked
    };
    if !target.is_file() && !target.is_symlink() {
        eprintln!("Cannot locate the installed executable.");
        return 1;
    }
    println!(
        "This will remove uninstall itself:\n  {}",
        sanitize(&target.display().to_string())
    );
    if !yes("Continue? [y/N] ") {
        println!("Cancelled.");
        return 0;
    }
    let command = vec![
        "rm".to_owned(),
        "--".to_owned(),
        target.display().to_string(),
    ];
    let writable = target.parent().is_some_and(|parent| {
        uninstall::command::run(
            "test",
            ["-w", &parent.display().to_string()],
            Duration::from_secs(2),
        )
        .ok()
    });
    let command = if writable {
        Ok(command)
    } else {
        removal::privilege(command)
    }
    .and_then(|command| removal::pin_command(&command));
    match command.and_then(|command| run_command(&command).map_err(|error| error.to_string())) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Could not remove uninstall: {}", sanitize(&error));
            1
        }
    }
}

fn no_match(query: &str) -> i32 {
    let lower = query.to_ascii_lowercase();
    if [
        ".rpm",
        ".deb",
        ".apk",
        ".ipk",
        ".opk",
        ".xbps",
        ".eopkg",
        ".txz",
        ".tgz",
        ".flatpak",
        ".flatpakref",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
        || lower.contains(".pkg.tar.")
    {
        println!(
            "No installed package matched this archive. The downloaded archive itself was not removed."
        );
    } else if let Some(path) = uninstall::command::which(query) {
        let absolute = uninstall::util::absolute_path(&path);
        let absolute_text = absolute.display().to_string();
        let base_package = if uninstall::discovery::rpm_manager() == Some("RPM-OSTree") {
            output("rpm", &["-qf", "--qf", "%{NAME}\n", "--", &absolute_text])
        } else {
            String::new()
        };
        if !base_package.trim().is_empty() {
            println!(
                "{} is part of the rpm-ostree base OS image, not a layered app. It was not offered as a normal removal.",
                sanitize(base_package.trim())
            );
            println!(
                "Changing the base image requires the advanced 'rpm-ostree override remove' workflow."
            );
        } else if !absolute.starts_with(uninstall::util::home())
            && !absolute.starts_with("/usr/local")
            && !absolute.starts_with("/opt")
        {
            println!(
                "The command exists at {}, but no supported package manager proved ownership.",
                sanitize(&absolute.display().to_string())
            );
            println!(
                "It was not offered as a standalone file because deleting system-directory commands directly is unsafe."
            );
        } else {
            println!("No likely installed apps found.");
        }
    } else {
        println!("No likely installed apps found.");
    }
    1
}

fn interactive(query: &str, show_dependencies: bool) -> i32 {
    let mut matches = progress("Checking installed applications…", || {
        discovery::find_matches(query)
    });
    provenance::annotate_roles(&mut matches);
    let (mut matches, hidden) = discovery::filter_dependencies(matches, query, show_dependencies);
    if hidden > 0 {
        println!(
            "Hidden {hidden} fuzzy dependency match{}; use --show-dependencies to include them.",
            if hidden == 1 { "" } else { "es" }
        );
    }
    if matches.is_empty() {
        return no_match(query);
    }
    progress(
        "Explaining installation and checking removal impact…",
        || provenance::decorate(&mut matches),
    );
    show_matches(&matches);
    let selected = choose(&matches);
    if selected.is_empty() {
        println!("Nothing selected.");
        return 0;
    }
    let mut plan = progress("Checking removal impact…", || {
        preview::build(&selected, &[])
    });
    show_impact(&plan, &selected);
    if plan.impact == Impact::Blocked {
        return 1;
    }

    let detected = cleanup::find_user_data(&selected);
    let options = cleanup::data_options(&selected, &detected);
    let data_choices = choose_data(&options);
    let cleanup_backends: Vec<String> = data_choices
        .iter()
        .filter_map(|index| options[*index].backend.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let chosen_paths: Vec<PathBuf> = data_choices
        .iter()
        .filter_map(|index| options[*index].path.clone())
        .collect();
    let data_candidates: Vec<CleanupCandidate> = match chosen_paths
        .iter()
        .map(|path| cleanup::snapshot(path))
        .collect()
    {
        Ok(candidates) => candidates,
        Err(error) => {
            eprintln!("Cannot safely select data: {}", sanitize(&error));
            return 1;
        }
    };
    let file_candidates: Vec<CleanupCandidate> = match file_targets(&selected)
        .iter()
        .map(|path| cleanup::snapshot(path))
        .collect()
    {
        Ok(candidates) => candidates,
        Err(error) => {
            eprintln!("Cannot safely select executable: {}", sanitize(&error));
            return 1;
        }
    };
    if !cleanup_backends.is_empty() {
        let final_plan = progress("Rechecking removal impact…", || {
            preview::build(&selected, &cleanup_backends)
        });
        if final_plan.fingerprint != plan.fingerprint {
            plan = final_plan;
            println!("\nThe cleanup choice changed the removal plan:");
            show_impact(&plan, &selected);
            if plan.impact == Impact::Blocked {
                return 1;
            }
        }
    }
    let exact = plan.status == PreviewStatus::Exact;
    let commands = match prepare_commands(&selected, &cleanup_backends, exact) {
        Ok(commands) => commands,
        Err(error) => {
            eprintln!("{}.", sanitize(&error));
            return 1;
        }
    };
    let (size, complete) = estimate(&selected, &plan, &options, &data_choices);
    println!("\n{}", ready_heading(size, complete));
    for command in &commands {
        println!("  {}", sanitize(&command_string(command)));
    }
    for path in &chosen_paths {
        println!(
            "  {}",
            sanitize(&command_string(&cleanup::cleanup_command(path)))
        );
    }
    let outside_home = chosen_paths
        .iter()
        .any(|path| !path.starts_with(uninstall::util::home()));
    let typed = matches!(plan.impact, Impact::High | Impact::Unknown) || outside_home;
    if typed {
        let expected = if selected.len() == 1 {
            format!("REMOVE {}", sanitize(&selected[0].name))
        } else {
            "REMOVE ALL".to_owned()
        };
        let label = if outside_home {
            "Data outside your home directory"
        } else if plan.impact == Impact::Unknown
            && selected.iter().all(|item| item.backend == "Standalone")
        {
            "Standalone removal"
        } else {
            match plan.impact {
                Impact::High => "High-impact transaction",
                _ => "Unknown-impact transaction",
            }
        };
        let answer =
            prompt(&format!("\n{label}. Type '{expected}' to continue: ")).unwrap_or_default();
        if answer != expected {
            println!("Cancelled.");
            return 0;
        }
    } else if !yes("\nContinue? [y/N] ") {
        println!("Cancelled.");
        return 0;
    }
    execute(
        &selected,
        &plan,
        &cleanup_backends,
        data_candidates,
        file_candidates,
    )
}

fn noninteractive(query: &str, backend: &str, confirmation: &str) -> i32 {
    let mut matches = discovery::find_matches(query);
    provenance::decorate(&mut matches);
    let selected: Vec<Match> = matches
        .into_iter()
        .filter(|item| {
            item.backend.eq_ignore_ascii_case(backend) && item.id.eq_ignore_ascii_case(query)
        })
        .collect();
    if selected.len() != 1 {
        eprintln!("Non-interactive removal requires one exact backend and package ID.");
        return 2;
    }
    let expected = format!("REMOVE {}:{}", selected[0].backend, selected[0].id);
    if confirmation != expected {
        eprintln!("Authorization must exactly equal: {expected}");
        return 2;
    }
    let plan = preview::build(&selected, &[]);
    if plan.status != PreviewStatus::Exact || !matches!(plan.impact, Impact::Low | Impact::Caution)
    {
        eprintln!(
            "Non-interactive removal refuses blocked, unknown, or high-impact transactions. Run interactively to review it."
        );
        return 1;
    }
    let file_candidates: Vec<CleanupCandidate> = match file_targets(&selected)
        .iter()
        .map(|path| cleanup::snapshot(path))
        .collect()
    {
        Ok(candidates) => candidates,
        Err(error) => {
            eprintln!("Cannot safely select executable: {}", sanitize(&error));
            return 1;
        }
    };
    execute(&selected, &plan, &[], Vec::new(), file_candidates)
}

fn run(cli: Cli) -> i32 {
    uninstall::command::set_debug(cli.debug);
    if rustix::process::geteuid().is_root()
        && std::env::var("SUDO_USER").is_ok_and(|user| !user.is_empty() && user != "root")
    {
        eprintln!("Run uninstall without sudo; it requests privilege only when needed.");
        return 2;
    }
    if cli.self_uninstall {
        if cli.app.is_some()
            || cli.show_dependencies
            || cli.json
            || cli.backend.is_some()
            || cli.confirm.is_some()
        {
            let _ = Cli::command()
                .error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "--self-uninstall cannot be combined with an app or another mode",
                )
                .print();
            return 2;
        }
        return self_uninstall();
    }
    if cli.json && cli.app.is_none() {
        let _ = Cli::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "--json requires an app query",
            )
            .print();
        return 2;
    }
    if cli.json && (cli.backend.is_some() || cli.confirm.is_some()) {
        let _ = Cli::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                "--json cannot be combined with --backend or --confirm",
            )
            .print();
        return 2;
    }
    if cli.backend.is_some() != cli.confirm.is_some() {
        let _ = Cli::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "--backend and --confirm must be used together",
            )
            .print();
        return 2;
    }
    if cli.backend.is_some() && cli.show_dependencies {
        let _ = Cli::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                "non-interactive removal cannot use --show-dependencies",
            )
            .print();
        return 2;
    }
    let query = cli.app.unwrap_or_else(|| {
        if io::stdin().is_terminal() {
            prompt("What app or command do you want to uninstall? ").unwrap_or_default()
        } else {
            String::new()
        }
    });
    if query.trim().is_empty() {
        println!("Nothing entered.");
        return 0;
    }
    if query.trim().eq_ignore_ascii_case("uninstall")
        && !cli.show_dependencies
        && !cli.json
        && cli.backend.is_none()
    {
        return self_uninstall();
    }
    if cli.json {
        return json_report(&query, cli.show_dependencies);
    }
    if let (Some(backend), Some(confirm)) = (cli.backend, cli.confirm) {
        return noninteractive(&query, &backend, &confirm);
    }
    interactive(&query, cli.show_dependencies)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = run(cli);
    ExitCode::from(u8::try_from(code.clamp(0, 255)).unwrap_or(1))
}
