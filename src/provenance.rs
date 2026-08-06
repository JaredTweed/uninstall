use crate::command::{exists, output, run};
use crate::model::{Backend, Match, Role};
use crate::platform::dnf_binary;
use crate::util::{package_base, sanitize};
use flate2::read::GzDecoder;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(45);

fn command_lines(program: &str, args: &[&str]) -> Option<HashSet<String>> {
    let result = run(program, args, TIMEOUT);
    result.ok().then(|| {
        result
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn dnf_reasons() -> &'static HashMap<String, (Role, String)> {
    static CACHE: OnceLock<HashMap<String, (Role, String)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let Some(manager) = dnf_binary() else {
            return HashMap::new();
        };
        if manager == "microdnf" {
            return HashMap::new();
        }
        let args = if manager == "dnf5" {
            vec![
                "repoquery",
                "--installed",
                "--queryformat=%{name}|%{reason}|%{from_repo}\n",
            ]
        } else {
            vec![
                "repoquery",
                "--installed",
                "--qf",
                "%{name}|%{reason}|%{from_repo}\n",
            ]
        };
        let result = run(manager, args, TIMEOUT);
        if !result.ok() {
            return HashMap::new();
        }
        result
            .stdout
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '|');
                let name = fields.next()?.trim();
                let reason = fields.next()?.trim();
                let repository = fields.next().unwrap_or_default().trim();
                let role = match reason.to_ascii_lowercase().as_str() {
                    "user" => Role::Explicit,
                    "external" | "external user" => Role::External,
                    "group" => Role::Group,
                    "dependency" => Role::Dependency,
                    "weak dependency" => Role::WeakDependency,
                    _ => Role::Unknown,
                };
                Some((name.to_owned(), (role, repository.to_owned())))
            })
            .collect()
    })
}

fn apt_auto() -> &'static Option<HashSet<String>> {
    static CACHE: OnceLock<Option<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        command_lines("apt-mark", &["showauto"])
            .map(|items| items.into_iter().map(|item| package_base(&item)).collect())
    })
}

fn zypper_userinstalled() -> &'static Option<HashSet<String>> {
    static CACHE: OnceLock<Option<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let result = run(
            "zypper",
            [
                "--no-refresh",
                "--xmlout",
                "packages",
                "--installed-only",
                "--userinstalled",
            ],
            TIMEOUT,
        );
        if !result.ok() {
            return None;
        }
        static EXPRESSION: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
            Regex::new(r#"(?i)<solvable\b[^>]*\bname=[\"']([^\"']+)[\"']"#)
                .expect("valid expression")
        });
        let mut names: HashSet<String> = EXPRESSION
            .captures_iter(&result.stdout)
            .map(|found| found[1].to_owned())
            .collect();
        if names.is_empty() {
            for line in result.stdout.lines() {
                let fields: Vec<&str> = line.split('|').map(str::trim).collect();
                if fields.len() >= 4 && fields[0].starts_with("i+") && !fields[2].is_empty() {
                    names.insert(fields[2].to_owned());
                }
            }
        }
        Some(names)
    })
}

fn yum_userinstalled() -> &'static Option<HashSet<String>> {
    static CACHE: OnceLock<Option<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        command_lines(
            "yum",
            &[
                "repoquery",
                "--installed",
                "--userinstalled",
                "--qf",
                "%{name}\n",
            ],
        )
    })
}

pub fn annotate_roles(items: &mut [Match]) {
    let dnf = items
        .iter()
        .any(|item| item.backend == Backend::Dnf)
        .then(dnf_reasons);
    let apt = (items.iter().any(|item| item.backend == Backend::Apt) && exists("apt-mark"))
        .then(|| apt_auto().as_ref())
        .flatten();
    let zypper = (items.iter().any(|item| item.backend == Backend::Zypper) && exists("zypper"))
        .then(|| zypper_userinstalled().as_ref())
        .flatten();
    let yum = items
        .iter()
        .any(|item| item.backend == Backend::Yum)
        .then(|| yum_userinstalled().as_ref())
        .flatten();
    for item in items {
        let base = package_base(&item.id);
        match item.backend.as_str() {
            "DNF" => {
                if let Some((role, repository)) = dnf.and_then(|reasons| reasons.get(&base)) {
                    item.role = *role;
                    if item.origin.is_empty() {
                        item.origin.clone_from(repository);
                    }
                }
            }
            "APT" => {
                if let Some(auto) = apt {
                    item.role = if auto.contains(&base) {
                        Role::Dependency
                    } else {
                        Role::Explicit
                    };
                }
            }
            "Zypper" => {
                if let Some(explicit) = zypper {
                    item.role = if explicit.contains(&base) {
                        Role::Explicit
                    } else {
                        Role::Dependency
                    };
                }
            }
            "YUM" => {
                if let Some(explicit) = yum {
                    item.role = if explicit.contains(&base) {
                        Role::Explicit
                    } else {
                        Role::Dependency
                    };
                }
            }
            _ => {}
        }
    }
}

fn read_history(path: &Path) -> String {
    if path.extension().is_some_and(|extension| extension == "gz") {
        let Ok(file) = File::open(path) else {
            return String::new();
        };
        let mut decoder = GzDecoder::new(file);
        let mut text = String::new();
        let _ = decoder.read_to_string(&mut text);
        text
    } else {
        fs::read(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }
}

fn sorted_logs(pattern_root: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(pattern_root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == prefix || name.starts_with(&format!("{prefix}.")))
        })
        .collect();
    files.sort_by(|left, right| right.cmp(left));
    files
}

fn compact_command(command: &str) -> String {
    let clean = sanitize(command)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.chars().count() <= 180 {
        clean
    } else {
        format!(
            "{}… (abbreviated)",
            clean.chars().take(176).collect::<String>()
        )
    }
}

fn apt_history(target: &str) -> Option<String> {
    let base = package_base(target);
    static FIELD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^(Start-Date|Commandline|Install|Remove|Purge):\s*(.+)$")
            .expect("valid expression")
    });
    let package = Regex::new(&format!(
        r"(?:^|[,\s]){}(?::[^\s,(]+)?(?:[:\s,(]|$)",
        regex::escape(&base)
    ))
    .expect("valid target expression");
    for path in sorted_logs(Path::new("/var/log/apt"), "history.log") {
        let text = read_history(&path);
        for block in text.rsplit("\n\n") {
            let mut command = "";
            let mut date = "";
            let mut event = false;
            for capture in FIELD.captures_iter(block) {
                match &capture[1] {
                    "Commandline" => command = capture.get(2).map_or("", |value| value.as_str()),
                    "Start-Date" => date = capture.get(2).map_or("", |value| value.as_str()),
                    "Install" => {
                        event |= package.is_match(capture.get(2).map_or("", |value| value.as_str()))
                    }
                    "Remove" | "Purge"
                        if package.is_match(capture.get(2).map_or("", |value| value.as_str())) =>
                    {
                        event = false;
                    }
                    _ => {}
                }
            }
            if event {
                let when = if date.is_empty() {
                    String::new()
                } else {
                    format!(" on {}", date.trim())
                };
                let command = if command.is_empty() {
                    "unknown command".to_owned()
                } else {
                    compact_command(command)
                };
                return Some(format!("APT history{when}: {command}"));
            }
        }
    }
    None
}

fn pacman_history(target: &str) -> Option<String> {
    let expression = Regex::new(&format!(
        r"^\[([^]]+)] \[ALPM] (installed|removed) {} \(([^)]+)\)",
        regex::escape(target)
    ))
    .expect("valid expression");
    let mut state: Option<String> = None;
    for path in sorted_logs(Path::new("/var/log"), "pacman.log")
        .into_iter()
        .rev()
    {
        for line in read_history(&path).lines() {
            if let Some(found) = expression.captures(line) {
                state = if &found[2] == "installed" {
                    Some(format!("Pacman log: installed {} on {}", target, &found[1]))
                } else {
                    None
                };
            }
        }
    }
    state
}

fn zypper_history(target: &str) -> Option<String> {
    let mut state = None;
    for line in read_history(Path::new("/var/log/zypp/history")).lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() < 3 || fields[2] != target {
            continue;
        }
        state = match fields[1] {
            "install" => Some(format!(
                "Zypper history: installed {target} on {}",
                fields[0]
            )),
            "remove" => None,
            _ => state,
        };
    }
    state
}

#[derive(Default)]
struct DnfRecord {
    id: String,
    command: String,
    reason: String,
    repository: String,
    groups: Vec<String>,
    environments: Vec<String>,
}

fn text_field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> &'a str {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str))
        .unwrap_or_default()
}

fn dnf_record(target: &str) -> Option<DnfRecord> {
    if dnf_binary() != Some("dnf5") {
        return legacy_dnf_history(target);
    }
    let option = format!("--contains-pkgs={}", package_base(target));
    let listed = run("dnf5", ["history", "list", &option, "--json"], TIMEOUT);
    let summaries: Vec<Value> = serde_json::from_str(&listed.stdout).ok()?;
    static NEVRA_EXPRESSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.*)-\d+:").expect("valid NEVRA expression"));
    for summary in summaries.iter().rev() {
        let id = summary.get("id").and_then(Value::as_i64)?;
        let result = run(
            "dnf5",
            ["history", "info", &id.to_string(), "--json"],
            TIMEOUT,
        );
        let details: Value = serde_json::from_str(&result.stdout).ok()?;
        let detail = details
            .as_array()
            .and_then(|array| array.first())
            .unwrap_or(&details);
        let object = detail.as_object()?;
        let packages = object
            .get("packages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for package in packages {
            let Some(package) = package.as_object() else {
                continue;
            };
            let name = text_field(package, &["name"]);
            let nevra = text_field(package, &["nevra"]);
            let nevra_name = NEVRA_EXPRESSION
                .captures(nevra)
                .map(|capture| capture[1].to_owned())
                .unwrap_or_default();
            if package_base(if name.is_empty() { &nevra_name } else { name })
                != package_base(target)
            {
                continue;
            }
            let action = text_field(package, &["action"]).to_ascii_lowercase();
            if !["install", "installed", "reinstall", "reinstalled"].contains(&action.as_str()) {
                continue;
            }
            let groups = object
                .get("groups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|group| group.as_object())
                .filter(|group| text_field(group, &["action"]).eq_ignore_ascii_case("install"))
                .map(|group| text_field(group, &["group", "id"]).to_owned())
                .filter(|group| !group.is_empty())
                .collect();
            let environments = object
                .get("environments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|environment| environment.as_object())
                .filter(|environment| {
                    text_field(environment, &["action"]).eq_ignore_ascii_case("install")
                })
                .map(|environment| text_field(environment, &["environment", "id"]).to_owned())
                .filter(|environment| !environment.is_empty())
                .collect();
            return Some(DnfRecord {
                id: id.to_string(),
                command: text_field(object, &["description", "command_line"])
                    .to_owned()
                    .or_else_empty(|| {
                        summary
                            .get("command_line")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    }),
                reason: text_field(package, &["reason"]).to_owned(),
                repository: text_field(package, &["repository", "repo_id"]).to_owned(),
                groups,
                environments,
            });
        }
    }
    None
}

trait OrElseEmpty {
    fn or_else_empty(self, fallback: impl FnOnce() -> String) -> String;
}
impl OrElseEmpty for String {
    fn or_else_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
    }
}

fn legacy_dnf_history(target: &str) -> Option<DnfRecord> {
    let manager = dnf_binary().unwrap_or("dnf");
    let result = run(manager, ["history", "list", target], TIMEOUT);
    static EXPRESSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^\s*(\d+)\s*\|").expect("valid expression"));
    let id = EXPRESSION
        .captures_iter(&result.stdout)
        .last()
        .map(|found| found[1].to_owned())?;
    let info = output(manager, &["history", "info", &id]);
    let command = info
        .lines()
        .find_map(|line| {
            line.split_once("Command Line :")
                .map(|(_, value)| value.trim().to_owned())
        })
        .unwrap_or_default();
    Some(DnfRecord {
        id,
        command,
        reason: String::new(),
        repository: String::new(),
        groups: Vec::new(),
        environments: Vec::new(),
    })
}

fn parse_key_records(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut records = Vec::new();
    let mut current = BTreeMap::new();
    let mut last = String::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !current.is_empty() {
                records.push(std::mem::take(&mut current));
            }
            last.clear();
        } else if !line.starts_with(char::is_whitespace) {
            if let Some((key, value)) = line.split_once(':') {
                last = key.trim().to_owned();
                current.insert(last.clone(), value.trim().to_owned());
            }
        } else if let Some(value) = current.get_mut(&last) {
            value.push(' ');
            value.push_str(line.trim());
        }
    }
    records
}

fn dnf_group_inventory(subject: &str) -> &'static [(String, String, Vec<String>)] {
    static GROUPS: OnceLock<Vec<(String, String, Vec<String>)>> = OnceLock::new();
    static ENVIRONMENTS: OnceLock<Vec<(String, String, Vec<String>)>> = OnceLock::new();
    let cache = if subject == "group" {
        &GROUPS
    } else {
        &ENVIRONMENTS
    };
    cache.get_or_init(|| dnf_group_inventory_uncached(subject))
}

fn dnf_group_inventory_uncached(subject: &str) -> Vec<(String, String, Vec<String>)> {
    if dnf_binary() != Some("dnf5") {
        return Vec::new();
    }
    let mut args = vec!["-q", "-C", subject, "info", "--installed"];
    if subject == "group" {
        args.push("--hidden");
    }
    parse_key_records(&output("dnf5", &args))
        .into_iter()
        .filter_map(|record| {
            let id = record.get("Id")?.clone();
            let name = record.get("Name").cloned().unwrap_or_else(|| id.clone());
            let keys: &[&str] = if subject == "group" {
                &[
                    "Mandatory packages",
                    "Default packages",
                    "Optional packages",
                    "Conditional packages",
                ]
            } else {
                &["Required groups", "Optional groups"]
            };
            let members = keys
                .iter()
                .flat_map(|key| {
                    record
                        .get(*key)
                        .into_iter()
                        .flat_map(|value| value.split_whitespace())
                })
                .filter(|value| *value != ":")
                .map(str::to_owned)
                .collect();
            Some((id, name, members))
        })
        .collect()
}

fn dnf_group_reason(target: &str, record: Option<&DnfRecord>) -> String {
    let transaction = record
        .map(|record| {
            format!(
                "DNF transaction {}: {}",
                record.id,
                compact_command(if record.command.is_empty() {
                    "unknown command"
                } else {
                    &record.command
                })
            )
        })
        .unwrap_or_default();
    let Some(record) = record else {
        return "installed as part of a package group".to_owned();
    };
    let memberships: Vec<_> = dnf_group_inventory("group")
        .iter()
        .filter(|(id, _, members)| {
            record.groups.contains(id) && members.iter().any(|member| member == target)
        })
        .collect();
    if memberships.len() != 1 {
        return if transaction.is_empty() {
            "installed as part of a package group".to_owned()
        } else {
            format!("installed as part of a package group; {transaction}")
        };
    }
    let (group_id, group_name, _) = &memberships[0];
    let environments: Vec<_> = dnf_group_inventory("environment")
        .iter()
        .filter(|(id, _, groups)| record.environments.contains(id) && groups.contains(group_id))
        .collect();
    let group = format!("{group_name} ({group_id})");
    let cause = if environments.len() == 1 {
        let mut environment = environments[0].1.clone();
        if !environment.to_ascii_lowercase().ends_with("environment") {
            environment.push_str(" Environment");
        }
        format!("installed through {environment} → {group}")
    } else {
        format!("installed as part of {group}")
    };
    if transaction.is_empty() {
        cause
    } else {
        format!("{cause}; {transaction}")
    }
}

fn root_path<F>(target: &str, explicit: &HashSet<String>, mut parents: F) -> Option<Vec<String>>
where
    F: FnMut(&str, bool) -> Option<HashSet<String>>,
{
    let mut queue = VecDeque::from([vec![target.to_owned()]]);
    let mut visited = HashSet::from([target.to_owned()]);
    while let Some(path) = queue.pop_front() {
        if path.len() > 10 || visited.len() > 64 {
            continue;
        }
        let current = path.last()?;
        for parent in parents(current, path.len() == 1)
            .unwrap_or_default()
            .into_iter()
            .map(|name| package_base(&name))
        {
            if parent == target || path.contains(&parent) {
                continue;
            }
            let mut next = path.clone();
            next.push(parent.clone());
            if explicit.contains(&parent) {
                next.reverse();
                return Some(next);
            }
            if visited.insert(parent) {
                queue.push_back(next);
            }
        }
    }
    None
}

fn render_dependency_path(path: &[String], relation: &str) -> String {
    if path.len() == 2 {
        format!("{} {relation} it", path[0])
    } else {
        format!(
            "{} ultimately {relation} it",
            path[..path.len() - 1].join(" → ")
        )
    }
}

fn dnf_parent(target: &str, weak: bool) -> Option<String> {
    let manager = dnf_binary()?;
    let explicit: HashSet<String> = dnf_reasons()
        .iter()
        .filter(|(_, (role, _))| matches!(role, Role::Explicit | Role::External | Role::Group))
        .map(|(name, _)| name.clone())
        .collect();
    let format = if manager == "dnf5" {
        "--queryformat=%{name}\n"
    } else {
        "--qf=%{name}\n"
    };
    let path = root_path(target, &explicit, |package, first| {
        let relationship = if weak && first {
            "--whatrecommends"
        } else {
            "--whatrequires"
        };
        command_lines(
            manager,
            &["repoquery", "--installed", relationship, package, format],
        )
    })?;
    Some(render_dependency_path(
        &path,
        if weak { "recommends" } else { "requires" },
    ))
}

fn apt_parent(target: &str) -> Option<String> {
    let explicit: HashSet<String> = command_lines("apt-mark", &["showmanual"])?
        .into_iter()
        .map(|name| package_base(&name))
        .collect();
    let path = root_path(target, &explicit, |package, _| {
        let text = output("apt-cache", &["rdepends", "--installed", package]);
        Some(
            text.lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty()
                        && !line.ends_with(':')
                        && !line.starts_with('|')
                        && !line.starts_with("Reverse Depends")
                })
                .map(package_base)
                .collect(),
        )
    })?;
    Some(render_dependency_path(&path, "requires"))
}

fn pacman_parent(target: &str) -> Option<String> {
    let explicit = command_lines("pacman", &["-Qqe"])?;
    let path = root_path(target, &explicit, |package, _| {
        let text = output("pacman", &["-Qi", package]);
        let required = parse_key_records(&text)
            .first()
            .and_then(|record| record.get("Required By"))
            .cloned()
            .unwrap_or_default();
        Some(
            required
                .split_whitespace()
                .filter(|name| *name != "None")
                .map(str::to_owned)
                .collect(),
        )
    })?;
    Some(render_dependency_path(&path, "requires"))
}

fn dependency_cause(item: &Match) -> Option<String> {
    match item.backend.as_str() {
        "DNF" => dnf_parent(&package_base(&item.id), item.role == Role::WeakDependency),
        "APT" => apt_parent(&package_base(&item.id)),
        "Pacman" => pacman_parent(&item.id),
        "APK" => command_lines("apk", &["info", "--rdepends", &item.id])
            .and_then(|parents| parents.into_iter().find(|name| name != &item.id))
            .map(|parent| format!("{parent} requires it")),
        "XBPS" => command_lines("xbps-query", &["--revdeps", &item.id])
            .and_then(|parents| parents.into_iter().next())
            .map(|parent| {
                format!(
                    "{} requires it",
                    parent.split('-').next().unwrap_or(&parent)
                )
            }),
        "Eopkg" if !item.origin.is_empty() => Some(format!("{} requires it", item.origin)),
        _ => None,
    }
}

fn dnf_history_text(record: Option<&DnfRecord>) -> Option<String> {
    let record = record?;
    let repository = if !record.repository.is_empty() && !record.repository.starts_with('@') {
        format!("; source repository: {}", record.repository)
    } else {
        String::new()
    };
    let reason = if record.reason.is_empty() {
        String::new()
    } else {
        format!("recorded reason: {}{repository}", record.reason)
    };
    let command = compact_command(if record.command.is_empty() {
        "unknown command"
    } else {
        &record.command
    });
    let detail = if reason.is_empty() {
        String::new()
    } else {
        format!(" ({reason})")
    };
    Some(format!("DNF transaction {}: {command}{detail}", record.id))
}

pub fn install_reason(item: &Match) -> String {
    if !item.reason.is_empty() {
        return item.reason.clone();
    }
    let history = match item.backend.as_str() {
        "APT" => apt_history(&item.id),
        "Pacman" => pacman_history(&item.id),
        "Zypper" => zypper_history(&item.id),
        _ => None,
    };
    if item.backend == Backend::Dnf {
        let record = dnf_record(&item.id);
        if item.role == Role::Group {
            return dnf_group_reason(&package_base(&item.id), record.as_ref());
        }
        let cause = match item.role {
            Role::Explicit => "explicitly requested".to_owned(),
            Role::External => {
                "installed outside DNF and later recorded in its package database".to_owned()
            }
            Role::Dependency | Role::WeakDependency => dependency_cause(item)
                .unwrap_or_else(|| format!("installed as a {}", item.role.label())),
            Role::Unknown => {
                "installed in the RPM database; DNF's original reason is unavailable".to_owned()
            }
            Role::Group => unreachable!(),
        };
        return dnf_history_text(record.as_ref())
            .map_or(cause.clone(), |event| format!("{cause}; {event}"));
    }
    let cause = match item.backend.as_str() {
        "Flatpak" => if item.origin.is_empty() { "explicitly requested through Flatpak".to_owned() } else { format!("explicitly requested through Flatpak remote {}", item.origin) },
        "Snap" => {
            let info = output("snap", &["info", &item.id]);
            let channel = info.lines().find_map(|line| line.trim().strip_prefix("tracking:").map(str::trim)).unwrap_or_default();
            if channel.is_empty() { "explicitly requested through Snap; original install event is unavailable".to_owned() } else { format!("explicitly requested through Snap channel {channel}; original install event is unavailable") }
        },
        "Gear Lever" => if item.origin.is_empty() { format!("managed by Gear Lever at {}; original download source is unavailable", item.id) } else { format!("managed by Gear Lever at {}; update source: {}", item.id, item.origin) },
        "AppImage" => format!("unmanaged AppImage at {}; its original download source is unknown", item.id),
        "Standalone" => format!("unmanaged executable at {}; no supported installer owns it and its original source is unknown", item.id),
        "Container Export" => format!("exported from container {}; the application is installed inside that container", item.profile),
        "Homebrew" | "Homebrew Cask" => {
            let state = if item.role == Role::Dependency { "installed by Homebrew as a dependency" } else { "explicitly requested through Homebrew" };
            if item.origin.is_empty() { state.to_owned() } else { format!("{state} from tap {}", item.origin) }
        },
        "Cargo" => if item.origin.is_empty() { "explicitly installed with cargo install; source metadata is unavailable".to_owned() } else { format!("explicitly installed with cargo install from {}", item.origin) },
        "Pipx" => format!("explicitly installed by pipx from {}", if item.origin.is_empty() { item.id.as_str() } else { item.origin.as_str() }),
        "UV Tool" => "explicitly installed as a uv tool; uv does not retain the original command".to_owned(),
        "NPM" => if item.origin.is_empty() { "explicitly installed in npm's global prefix; resolved source is unavailable".to_owned() } else { format!("explicitly installed globally by npm from {}", item.origin) },
        "Nix" => format!("explicit profile entry{}; Nix profile metadata is authoritative", if item.profile.is_empty() { String::new() } else { format!(" in {}", item.profile) }),
        "Nix Legacy" => "explicit entry in the legacy Nix user environment".to_owned(),
        "Guix" => format!("explicit entry in Guix profile {}", item.profile),
        "Conda" | "Micromamba" => format!("installed in {} environment {}{}; metadata does not retain whether it was directly requested", item.backend, item.profile, if item.origin.is_empty() { String::new() } else { format!(" from channel {}", item.origin) }),
        "RPM-OSTree" => "explicitly layered onto the current rpm-ostree deployment".to_owned(),
        "RPM" => "present in the RPM database; no supported transaction history identifies the original request".to_owned(),
        "APT-RPM" => "present in the RPM database managed by APT-RPM; original install history is unavailable".to_owned(),
        "Slackware" => "present in Slackware's installed-package log; pkgtools does not record dependency reasons".to_owned(),
        "Swupd" | "Swupd 3rd-party" => if item.role == Role::Dependency { "installed because another Swupd bundle includes it".to_owned() } else { "explicitly tracked as an installed Swupd bundle".to_owned() },
        "APK" if item.role == Role::Explicit => "listed in /etc/apk/world as a top-level constraint; APK does not retain the original install command".to_owned(),
        "APK" => dependency_cause(item).map_or_else(|| "not listed in /etc/apk/world and retained by APK as a dependency; its original parent is unavailable".to_owned(), |parent| format!("{parent}; it is not listed in /etc/apk/world")),
        "OPKG" if item.role == Role::Explicit => "recorded as manually installed in OPKG's status database; original install history is unavailable".to_owned(),
        "OPKG" if item.role == Role::Dependency => "recorded as automatically installed in OPKG's status database; the original parent is unavailable".to_owned(),
        "XBPS" if item.role == Role::Explicit => "listed by XBPS as a manually installed package; original transaction history is unavailable".to_owned(),
        "XBPS" => dependency_cause(item).map_or_else(|| "not listed by XBPS as manually installed; the original parent is unavailable".to_owned(), |parent| format!("{parent}; it is not listed as manually installed")),
        "Portage" if item.role == Role::Explicit => format!("listed in Portage's @world set{}; Portage does not retain the original emerge command", if item.origin.is_empty() { String::new() } else { format!(" from repository {}", item.origin) }),
        "Portage" => format!("not listed in Portage's @world set and retained as a dependency{}; the original parent and emerge command are unavailable", if item.origin.is_empty() { String::new() } else { format!(" from repository {}", item.origin) }),
        "Eopkg" if item.role == Role::Explicit => "not marked automatic by Eopkg, so it is currently a top-level package; original install history is unavailable".to_owned(),
        "Eopkg" => dependency_cause(item).unwrap_or_else(|| "marked automatic by Eopkg; the original parent is unavailable".to_owned()),
        _ if item.role.is_dependency() => dependency_cause(item).unwrap_or_else(|| format!("marked as a {} by {}", item.role.label(), item.backend)),
        _ if item.role == Role::Group => format!("installed as part of a {} package group", item.backend),
        _ if item.role == Role::Explicit => format!("explicitly requested through {}; original install history is unavailable", item.backend),
        _ => format!("installed through {}; its original install reason is unavailable", item.backend),
    };
    history.map_or(cause.clone(), |event| format!("{cause}; {event}"))
}

pub fn decorate(items: &mut [Match]) {
    annotate_roles(items);
    explain(items);
}

pub fn explain(items: &mut [Match]) {
    for item in items {
        item.reason = install_reason(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_reason_is_explicit_about_unknown_source() {
        let item = Match::new(Backend::Standalone, "/home/me/.local/bin/edit", "edit");
        assert!(install_reason(&item).contains("original source is unknown"));
    }

    #[test]
    fn command_abbreviation_is_marked() {
        let command = "dnf install ".to_owned() + &"package ".repeat(50);
        assert!(compact_command(&command).ends_with("(abbreviated)"));
    }

    #[test]
    fn dependency_search_reaches_an_explicit_root() {
        let explicit = HashSet::from(["application".to_owned()]);
        let path = root_path("leaf", &explicit, |package, _| {
            Some(match package {
                "leaf" => HashSet::from(["middle".to_owned()]),
                "middle" => HashSet::from(["application".to_owned()]),
                _ => HashSet::new(),
            })
        })
        .expect("root path");
        assert_eq!(path, ["application", "middle", "leaf"]);
        assert_eq!(
            render_dependency_path(&path, "requires"),
            "application → middle ultimately requires it"
        );
    }

    #[test]
    fn dependency_search_ignores_cycles() {
        let explicit = HashSet::new();
        assert!(
            root_path("one", &explicit, |package, _| Some(HashSet::from([
                if package == "one" { "two" } else { "one" }.to_owned()
            ])))
            .is_none()
        );
    }
}
