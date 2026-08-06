use crate::command::{exists, output, run, which};
use crate::model::{Backend, Match, Role};
use crate::platform::{NativeFamily, native_family, rpm_manager};
use crate::util::{
    QueryMatcher, absolute_path, home, norm, package_base, package_base_ref, parse_size,
    path_within,
};
use quick_xml::Reader;
use quick_xml::events::Event;
use rayon::prelude::*;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;
use walkdir::WalkDir;

type OwnerResult = Arc<OnceLock<Vec<Match>>>;
type OwnerCache = Mutex<HashMap<String, OwnerResult>>;

fn read(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

fn detect_flatpak(query: &str) -> Vec<Match> {
    if !exists("flatpak") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let mut locations = vec![
        ("user".to_owned(), String::new()),
        ("system".to_owned(), String::new()),
    ];
    let installation_dir = Path::new("/etc/flatpak/installations.d");
    if let Ok(entries) = fs::read_dir(installation_dir) {
        let section = Regex::new(r#"(?i)^\[Installation\s+\"([A-Za-z0-9_.-]+)\"\]$"#)
            .expect("valid Flatpak section expression");
        for entry in entries.flatten() {
            let text = read(&entry.path());
            for line in text.lines() {
                if let Some(found) = section.captures(line.trim()) {
                    locations.push(("system".to_owned(), found[1].to_owned()));
                }
            }
        }
    }
    locations.sort();
    locations.dedup();
    let mut found = Vec::new();
    for (scope, installation) in locations {
        let location = if installation.is_empty() {
            format!("--{scope}")
        } else {
            format!("--installation={installation}")
        };
        let text = output(
            "flatpak",
            &[
                "list",
                "--app",
                &location,
                "--columns=application,name,version,origin,size",
            ],
        );
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 2 || !matcher.relevant(&[fields[0], fields[1]]) {
                continue;
            }
            let mut item = Match::new(Backend::Flatpak, fields[0], fields[1]);
            item.version = fields.get(2).copied().unwrap_or_default().to_owned();
            item.origin = fields.get(3).copied().unwrap_or_default().to_owned();
            item.installed_size_bytes = fields.get(4).and_then(|size| parse_size(size));
            item.scope = scope.clone();
            item.installation = installation.clone();
            item.role = Role::Explicit;
            found.push(item);
        }
    }
    found
}

fn detect_snap(query: &str) -> Vec<Match> {
    if !exists("snap") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    output("snap", &["list"])
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 2 || !matcher.relevant(&[fields[0]]) {
                return None;
            }
            let mut item = Match::new(Backend::Snap, fields[0], fields[0]);
            item.version = fields[1].to_owned();
            item.role = Role::Explicit;
            Some(item)
        })
        .collect()
}

fn apt_inventory_uncached() -> Vec<Match> {
    if !exists("dpkg-query") {
        return Vec::new();
    }
    let format = "${db:Status-Abbrev}\t${binary:Package}\t${Version}\t${binary:Summary}\t${Installed-Size}\n";
    output("dpkg-query", &["-W", &format!("-f={format}")])
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.splitn(5, '\t').collect();
            if fields.len() != 5 || fields[0].as_bytes().get(1) != Some(&b'i') {
                return None;
            }
            let mut item = Match::new(Backend::Apt, fields[1], fields[1]);
            item.version = fields[2].to_owned();
            item.summary = fields[3].to_owned();
            item.installed_size_bytes = fields[4].parse::<u64>().ok().map(|size| size * 1024);
            Some(item)
        })
        .collect()
}

fn apt_inventory() -> &'static [Match] {
    static CACHE: OnceLock<Vec<Match>> = OnceLock::new();
    CACHE.get_or_init(apt_inventory_uncached)
}

fn detect_apt(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    apt_inventory()
        .iter()
        .filter(|item| matcher.relevant(&[&item.id, &item.summary]))
        .cloned()
        .collect()
}

fn rpm_layered() -> Option<HashSet<String>> {
    if rpm_manager() != Some("RPM-OSTree") {
        return None;
    }
    let value: Value = serde_json::from_str(&output("rpm-ostree", &["status", "--json"])).ok()?;
    let deployment = value.get("deployments")?.as_array()?.first()?;
    let mut layered = HashSet::new();
    for key in ["requested-packages", "requested-local-packages"] {
        if let Some(values) = deployment.get(key).and_then(Value::as_array) {
            layered.extend(values.iter().filter_map(Value::as_str).map(package_base));
        }
    }
    Some(layered)
}

fn rpm_inventory_uncached() -> Vec<Match> {
    let Some(kind) = rpm_manager() else {
        return Vec::new();
    };
    let backend = Backend::parse(kind).expect("known RPM backend");
    if !exists("rpm") {
        return Vec::new();
    }
    let layered = rpm_layered();
    let format = "P\\t%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{SUMMARY}\\t%{SIZE}\\t%{ARCH}\\n";
    let mut found = Vec::new();
    for line in output("rpm", &["-qa", "--qf", format]).lines() {
        let fields: Vec<&str> = line.splitn(6, '\t').collect();
        if fields.len() != 6 || fields[0] != "P" {
            continue;
        }
        if layered
            .as_ref()
            .is_some_and(|items| !items.contains(fields[1]))
        {
            continue;
        }
        let id = if fields[5].is_empty() {
            fields[1].to_owned()
        } else {
            format!("{}.{}", fields[1], fields[5])
        };
        let mut item = Match::new(backend, id, fields[1]);
        item.version = fields[2].to_owned();
        item.summary = fields[3].to_owned();
        item.installed_size_bytes = fields[4].parse().ok();
        item.architecture = fields[5].to_owned();
        if kind == "RPM-OSTree" {
            item.role = Role::Explicit;
        }
        found.push(item);
    }
    found
}

fn rpm_inventory() -> &'static [Match] {
    static CACHE: OnceLock<Vec<Match>> = OnceLock::new();
    CACHE.get_or_init(rpm_inventory_uncached)
}

fn detect_rpm(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let mut records: Vec<Match> = rpm_inventory()
        .iter()
        .filter(|item| matcher.relevant(&[&item.name, &item.summary]) || matcher.exact(&item.id))
        .cloned()
        .collect();
    if records.iter().any(|item| matcher.exact(&item.id)) {
        records.retain(|item| matcher.exact(&item.id));
    }
    records
}

fn parse_key_value_records(text: &str) -> Vec<BTreeMap<String, String>> {
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

fn pacman_inventory_uncached() -> Vec<Match> {
    if !exists("pacman") {
        return Vec::new();
    }
    parse_key_value_records(&output("pacman", &["-Qi"]))
        .into_iter()
        .filter_map(|record| {
            let name = record.get("Name")?.clone();
            let mut item = Match::new(Backend::Pacman, &name, &name);
            item.version = record.get("Version").cloned().unwrap_or_default();
            item.summary = record.get("Description").cloned().unwrap_or_default();
            item.installed_size_bytes = record
                .get("Installed Size")
                .and_then(|size| parse_size(size));
            item.role = if record
                .get("Install Reason")
                .is_some_and(|reason| reason.to_ascii_lowercase().contains("dependency"))
            {
                Role::Dependency
            } else {
                Role::Explicit
            };
            Some(item)
        })
        .collect()
}

fn pacman_inventory() -> &'static [Match] {
    static CACHE: OnceLock<Vec<Match>> = OnceLock::new();
    CACHE.get_or_init(pacman_inventory_uncached)
}

fn detect_pacman(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    pacman_inventory()
        .iter()
        .filter(|item| matcher.relevant(&[&item.id, &item.summary]))
        .cloned()
        .collect()
}

#[derive(Debug, Clone)]
struct PackageRecord {
    name: String,
    version: String,
    summary: String,
    size: Option<u64>,
    role: Role,
    origin: String,
}

fn apk_inventory_uncached() -> Vec<PackageRecord> {
    if !exists("apk") {
        return Vec::new();
    }
    let world: HashSet<String> = read(Path::new("/etc/apk/world"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.split(['<', '>', '=', '~', '@'])
                .next()
                .unwrap_or(line)
                .to_owned()
        })
        .collect();
    let text = read(Path::new("/lib/apk/db/installed"));
    if !text.is_empty() {
        return text
            .split("\n\n")
            .filter_map(|block| {
                let fields: HashMap<&str, &str> = block
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .collect();
                let name = fields.get("P")?.to_string();
                Some(PackageRecord {
                    role: if world.contains(&name) {
                        Role::Explicit
                    } else {
                        Role::Dependency
                    },
                    name,
                    version: fields.get("V").copied().unwrap_or_default().to_owned(),
                    summary: fields.get("T").copied().unwrap_or_default().to_owned(),
                    size: fields.get("I").and_then(|size| size.parse().ok()),
                    origin: fields.get("o").copied().unwrap_or_default().to_owned(),
                })
            })
            .collect();
    }
    static EXPRESSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+)-([0-9][^\s]*-r\d+)$").expect("valid APK expression"));
    output("apk", &["info", "--verbose"])
        .lines()
        .filter_map(|line| EXPRESSION.captures(line.trim()))
        .map(|found| PackageRecord {
            name: found[1].to_owned(),
            version: found[2].to_owned(),
            summary: String::new(),
            size: None,
            role: Role::Unknown,
            origin: String::new(),
        })
        .collect()
}

fn apk_inventory() -> &'static [PackageRecord] {
    static CACHE: OnceLock<Vec<PackageRecord>> = OnceLock::new();
    CACHE.get_or_init(apk_inventory_uncached)
}

fn detect_apk(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    apk_inventory()
        .iter()
        .filter(|record| matcher.relevant(&[&record.name, &record.summary]))
        .map(|record| package_record_match(Backend::Apk, record))
        .collect()
}

fn opkg_inventory_uncached() -> Vec<PackageRecord> {
    if !exists("opkg") {
        return Vec::new();
    }
    let status = ["/usr/lib/opkg/status", "/var/lib/opkg/status"]
        .into_iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .map(read)
        .unwrap_or_default();
    if !status.is_empty() {
        return status
            .split("\n\n")
            .filter_map(|block| {
                let fields: HashMap<&str, &str> = block
                    .lines()
                    .filter_map(|line| line.split_once(": "))
                    .collect();
                let name = fields.get("Package")?.to_string();
                if !fields
                    .get("Status")
                    .is_some_and(|value| value.contains("installed"))
                {
                    return None;
                }
                Some(PackageRecord {
                    role: if fields.get("Auto-Installed") == Some(&"yes") {
                        Role::Dependency
                    } else {
                        Role::Explicit
                    },
                    name,
                    version: fields
                        .get("Version")
                        .copied()
                        .unwrap_or_default()
                        .to_owned(),
                    summary: fields
                        .get("Description")
                        .copied()
                        .unwrap_or_default()
                        .to_owned(),
                    size: fields
                        .get("Installed-Size")
                        .and_then(|value| value.parse().ok()),
                    origin: String::new(),
                })
            })
            .collect();
    }
    output("opkg", &["list-installed"])
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.splitn(3, " - ").collect();
            (fields.len() >= 2).then(|| PackageRecord {
                name: fields[0].to_owned(),
                version: fields[1].to_owned(),
                summary: fields.get(2).copied().unwrap_or_default().to_owned(),
                size: None,
                role: Role::Unknown,
                origin: String::new(),
            })
        })
        .collect()
}

fn opkg_inventory() -> &'static [PackageRecord] {
    static CACHE: OnceLock<Vec<PackageRecord>> = OnceLock::new();
    CACHE.get_or_init(opkg_inventory_uncached)
}

fn records_to_matches(query: &str, backend: Backend, records: &[PackageRecord]) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    records
        .iter()
        .filter(|record| matcher.relevant(&[&record.name, &record.summary]))
        .map(|record| package_record_match(backend, record))
        .collect()
}

fn package_record_match(backend: Backend, record: &PackageRecord) -> Match {
    let mut item = Match::new(backend, &record.name, &record.name);
    item.version.clone_from(&record.version);
    item.summary.clone_from(&record.summary);
    item.installed_size_bytes = record.size;
    item.role = record.role;
    item.origin.clone_from(&record.origin);
    item
}

fn detect_opkg(query: &str) -> Vec<Match> {
    records_to_matches(query, Backend::Opkg, opkg_inventory())
}

fn split_xbps(value: &str) -> (String, String) {
    static EXPRESSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+)-([0-9][^\s]*_\d+)$").expect("valid XBPS expression"));
    EXPRESSION.captures(value.trim()).map_or_else(
        || (value.trim().to_owned(), String::new()),
        |found| (found[1].to_owned(), found[2].to_owned()),
    )
}

fn xbps_inventory_uncached() -> Vec<PackageRecord> {
    if !exists("xbps-query") {
        return Vec::new();
    }
    let manual: HashSet<String> = output("xbps-query", &["--list-manual-pkgs"])
        .lines()
        .map(|line| split_xbps(line.split_whitespace().next().unwrap_or_default()).0)
        .collect();
    output("xbps-query", &["--list-pkgs"])
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line
                .splitn(3, char::is_whitespace)
                .filter(|value| !value.is_empty())
                .collect();
            if fields.len() < 2 || fields[0] != "ii" {
                return None;
            }
            let (name, version) = split_xbps(fields[1]);
            Some(PackageRecord {
                role: if manual.contains(&name) {
                    Role::Explicit
                } else {
                    Role::Dependency
                },
                name,
                version,
                summary: fields.get(2).copied().unwrap_or_default().to_owned(),
                size: None,
                origin: String::new(),
            })
        })
        .collect()
}

fn xbps_inventory() -> &'static [PackageRecord] {
    static CACHE: OnceLock<Vec<PackageRecord>> = OnceLock::new();
    CACHE.get_or_init(xbps_inventory_uncached)
}

fn detect_xbps(query: &str) -> Vec<Match> {
    records_to_matches(query, Backend::Xbps, xbps_inventory())
}

fn detect_portage(query: &str) -> Vec<Match> {
    if !exists("emerge") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let prefix = std::env::var_os("EPREFIX").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let root = prefix.join("var/db/pkg");
    let world = read(&prefix.join("var/lib/portage/world"));
    let mut found = Vec::new();
    let categories = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return found,
    };
    for category in categories.flatten().filter(|entry| entry.path().is_dir()) {
        let Ok(packages) = fs::read_dir(category.path()) else {
            continue;
        };
        for package in packages.flatten().filter(|entry| entry.path().is_dir()) {
            let name = read(&package.path().join("PN")).trim().to_owned();
            if name.is_empty() {
                continue;
            }
            let id = format!("{}/{}", category.file_name().to_string_lossy(), name);
            let summary = read(&package.path().join("DESCRIPTION")).trim().to_owned();
            if !matcher.relevant(&[&id, &name, &summary]) {
                continue;
            }
            let mut item = Match::new(Backend::Portage, id, name);
            item.version = read(&package.path().join("PVR")).trim().to_owned();
            item.summary = summary;
            item.origin = read(&package.path().join("repository")).trim().to_owned();
            item.installed_size_bytes = read(&package.path().join("SIZE")).trim().parse().ok();
            item.role = if portage_world_contains(&world, &item.id) {
                Role::Explicit
            } else {
                Role::Dependency
            };
            item.scope = if path_within(&prefix, &home()) {
                "user"
            } else {
                "system"
            }
            .to_owned();
            found.push(item);
        }
    }
    found
}

fn portage_world_contains(world: &str, id: &str) -> bool {
    world.lines().any(|line| {
        let mut atom = line.trim();
        if atom.is_empty() || atom.starts_with(['#', '@']) {
            return false;
        }
        atom = atom.trim_start_matches(['<', '>', '=', '~', '!']);
        atom = atom.split(['[', ':']).next().unwrap_or(atom);
        atom == id
            || atom.strip_prefix(id).is_some_and(|suffix| {
                suffix.starts_with('-') && suffix[1..].starts_with(char::is_numeric)
            })
    })
}

fn detect_slackware(query: &str) -> Vec<Match> {
    if !exists("removepkg") {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir("/var/log/packages") else {
        return Vec::new();
    };
    static EXPRESSION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(.+)-([^-]+)-([^-]+)-([^-]+)$").expect("valid Slackware expression")
    });
    let matcher = QueryMatcher::new(query);
    entries
        .flatten()
        .filter_map(|entry| {
            let package = entry.file_name().to_string_lossy().into_owned();
            let capture = EXPRESSION.captures(&package);
            let name = capture.as_ref().map_or(package.as_str(), |value| &value[1]);
            if !matcher.relevant(&[name]) {
                return None;
            }
            let mut item = Match::new(Backend::Slackware, name, name);
            item.version = capture.map_or_else(String::new, |value| value[2].to_owned());
            Some(item)
        })
        .collect()
}

fn detect_eopkg(query: &str) -> Vec<Match> {
    if !exists("eopkg") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    eopkg_inventory()
        .iter()
        .filter(|record| matcher.relevant(&[&record.name, &record.summary]))
        .cloned()
        .map(|record| {
            let mut item = Match::new(Backend::Eopkg, &record.name, &record.name);
            item.version = record.version;
            item.role = record.role;
            item.summary = record.summary;
            item.origin = record.origin;
            item
        })
        .collect()
}

fn eopkg_inventory_uncached() -> Vec<PackageRecord> {
    let automatic = output("eopkg", &["--no-color", "list-installed", "--automatic"]);
    let automatic: HashMap<&str, &str> = automatic
        .lines()
        .filter_map(|line| line.split_once(" - "))
        .map(|(name, parent)| (name.trim(), parent.trim()))
        .filter(|(name, _)| !name.is_empty())
        .collect();
    output("eopkg", &["--no-color", "list-installed"])
        .lines()
        .filter_map(|line| {
            let (name, summary) = line.split_once(" - ")?;
            let name = name.trim();
            (!name.is_empty()).then_some(PackageRecord {
                name: name.to_owned(),
                version: String::new(),
                summary: summary.trim().to_owned(),
                size: None,
                role: if automatic.contains_key(name) {
                    Role::Dependency
                } else {
                    Role::Explicit
                },
                origin: automatic.get(name).copied().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

fn eopkg_inventory() -> &'static [PackageRecord] {
    static CACHE: OnceLock<Vec<PackageRecord>> = OnceLock::new();
    CACHE.get_or_init(eopkg_inventory_uncached)
}

fn detect_swupd(query: &str) -> Vec<Match> {
    if !exists("swupd") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let result = run(
        "swupd",
        ["bundle-list", "--status", "--quiet"],
        Duration::from_secs(30),
    );
    let mut found = Vec::new();
    for line in result.combined().lines() {
        let Some((name, detail)) = line.split_once(':') else {
            continue;
        };
        if !matcher.relevant(&[name]) {
            continue;
        }
        let mut item = Match::new(Backend::Swupd, name.trim(), name.trim());
        item.role = if detail.to_ascii_lowercase().contains("explicit") {
            Role::Explicit
        } else {
            Role::Dependency
        };
        found.push(item);
    }
    found
}

fn detect_swupd_third_party(query: &str) -> Vec<Match> {
    if !exists("swupd") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let list = run("swupd", ["3rd-party", "list"], Duration::from_secs(30));
    if !list.ok() {
        return Vec::new();
    }
    static REPOSITORY_EXPRESSION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:repo|repository)\s*[: ]\s*([A-Za-z0-9+_.-]+)")
            .expect("valid Swupd expression")
    });
    let repositories: BTreeSet<String> = list
        .combined()
        .lines()
        .filter_map(|line| {
            REPOSITORY_EXPRESSION
                .captures(line)
                .map(|value| value[1].to_owned())
        })
        .collect();
    let mut found = Vec::new();
    for repository in repositories {
        let result = run(
            "swupd",
            ["3rd-party", "bundle-list", "--repo", &repository],
            Duration::from_secs(30),
        );
        if !result.ok() {
            continue;
        }
        for id in result
            .stdout
            .lines()
            .filter_map(|line| line.split_whitespace().next())
        {
            if matcher.relevant(&[id]) {
                let mut item = Match::new(Backend::SwupdThirdParty, id, id);
                item.origin = repository.clone();
                item.role = Role::Explicit;
                found.push(item);
            }
        }
    }
    found
}

fn detect_homebrew(query: &str) -> Vec<Match> {
    if !exists("brew") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let value: Value = serde_json::from_str(&output("brew", &["info", "--json=v2", "--installed"]))
        .unwrap_or(Value::Null);
    let mut found = Vec::new();
    for (backend, key) in [
        (Backend::Homebrew, "formulae"),
        (Backend::HomebrewCask, "casks"),
    ] {
        for details in value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = details
                .get("name")
                .or_else(|| details.get("token"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            if !matcher.relevant(&[id]) {
                continue;
            }
            let installed = details
                .get("installed")
                .and_then(Value::as_array)
                .and_then(|items| items.last());
            let version = installed
                .and_then(|item| item.get("version"))
                .or_else(|| details.get("version"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut item = Match::new(backend, id, id);
            item.version = version.to_owned();
            item.origin = details
                .get("tap")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            item.role = match installed
                .and_then(|item| item.get("installed_on_request"))
                .and_then(Value::as_bool)
            {
                Some(true) => Role::Explicit,
                Some(false) => Role::Dependency,
                None => Role::Unknown,
            };
            item.scope = "user".to_owned();
            found.push(item);
        }
    }
    found
}

fn gearlever_command() -> Option<(String, Vec<String>)> {
    if exists("gearlever") {
        return Some(("gearlever".to_owned(), Vec::new()));
    }
    if exists("flatpak") {
        let known = output("flatpak", &["list", "--app", "--columns=application"])
            .lines()
            .any(|line| line.trim() == "it.mijorus.gearlever");
        if known {
            return Some((
                "flatpak".to_owned(),
                vec!["run".to_owned(), "it.mijorus.gearlever".to_owned()],
            ));
        }
    }
    None
}

fn detect_gearlever(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let Some((program, prefix)) = gearlever_command() else {
        return Vec::new();
    };
    let mut args = prefix.clone();
    args.extend(["--list-installed".to_owned(), "--json".to_owned()]);
    let references: Vec<&str> = args.iter().map(String::as_str).collect();
    let text = output(&program, &references);
    let json_line = text
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'));
    let value: Value = json_line
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(Value::Null);
    let mut found = Vec::new();
    for details in value
        .get("installed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = details.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(path_value) = details.get("path").and_then(Value::as_str) else {
            continue;
        };
        let path = PathBuf::from(path_value);
        if !path.is_absolute() || (!path.is_file() && !path.is_symlink()) {
            continue;
        }
        if !matcher.relevant(&[
            name,
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
            path_value,
        ]) {
            continue;
        }
        let mut item = Match::new(Backend::GearLever, path_value, name);
        item.source_path = Some(path);
        item.scope = "user".to_owned();
        item.version = details
            .get("current_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        item.origin = ["update_url", "source_url", "url"]
            .into_iter()
            .find_map(|key| details.get(key).and_then(Value::as_str))
            .unwrap_or_default()
            .to_owned();
        item.role = Role::Explicit;
        item.installed_size_bytes = item
            .source_path
            .as_ref()
            .and_then(|source| source.metadata().ok())
            .map(|metadata| metadata.len());
        found.push(item);
    }
    found
}

fn detect_pipx(query: &str) -> Vec<Match> {
    if !exists("pipx") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let mut found = Vec::new();
    for (scope, global) in [("user", false), ("system", true)] {
        let args = if global {
            vec!["list", "--json", "--global"]
        } else {
            vec!["list", "--json"]
        };
        let value: Value = serde_json::from_str(&output("pipx", &args)).unwrap_or(Value::Null);
        if let Some(venvs) = value.get("venvs").and_then(Value::as_object) {
            for (environment, metadata) in venvs {
                let main = metadata
                    .get("metadata")
                    .unwrap_or(metadata)
                    .get("main_package");
                let name = main
                    .and_then(|value| value.get("package"))
                    .and_then(Value::as_str)
                    .unwrap_or(environment);
                let apps: Vec<&str> = main
                    .and_then(|value| value.get("apps"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if !matcher.relevant(&[environment, name])
                    && !apps.iter().any(|app| matcher.relevant(&[*app]))
                {
                    continue;
                }
                let mut item = Match::new(Backend::Pipx, environment, name);
                item.version = main
                    .and_then(|value| value.get("package_version"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                item.origin = main
                    .and_then(|value| value.get("package_or_url"))
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .to_owned();
                item.scope = scope.to_owned();
                item.role = Role::Explicit;
                if let Some(app) = apps.iter().find(|app| matcher.relevant(&[*app])) {
                    item.command_path = which(app);
                }
                found.push(item);
            }
        }
    }
    found
}

fn detect_uv(query: &str) -> Vec<Match> {
    if !exists("uv") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let text = output("uv", &["tool", "list"]);
    let mut records: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) && !line.starts_with('-') {
            let fields: Vec<&str> = line.trim_end_matches(':').split_whitespace().collect();
            if let Some(name) = fields.first() {
                records.push((
                    (*name).to_owned(),
                    fields
                        .get(1)
                        .copied()
                        .unwrap_or_default()
                        .trim_start_matches('v')
                        .to_owned(),
                    Vec::new(),
                ));
            }
        } else if let Some(command) = line.trim().strip_prefix("- ") {
            if let Some(record) = records.last_mut() {
                record.2.push(
                    command
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
        }
    }
    let mut found = Vec::new();
    for (name, version, commands) in records {
        if !matcher.relevant(&[&name])
            && !commands.iter().any(|command| matcher.relevant(&[command]))
        {
            continue;
        }
        let mut item = Match::new(Backend::UvTool, &name, &name);
        item.version = version;
        item.scope = "user".to_owned();
        item.role = Role::Explicit;
        if let Some(command) = commands.iter().find(|command| matcher.relevant(&[command])) {
            item.command_path = which(command);
        }
        found.push(item);
    }
    found
}

fn npm_global_names(prefix: &Path) -> Option<Vec<String>> {
    let root = prefix.join("lib/node_modules");
    let entries = fs::read_dir(root).ok()?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('@') {
            let scoped = fs::read_dir(entry.path()).ok()?;
            for package in scoped {
                let package = package.ok()?;
                names.push(format!("{name}/{}", package.file_name().to_string_lossy()));
            }
        } else {
            names.push(name);
        }
    }
    Some(names)
}

fn detect_npm(query: &str) -> Vec<Match> {
    if !exists("npm") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let prefix_text = output("npm", &["prefix", "--global"]);
    let prefix = PathBuf::from(prefix_text.trim());
    if !prefix.as_os_str().is_empty()
        && npm_global_names(&prefix)
            .is_some_and(|names| !names.iter().any(|name| matcher.relevant(&[name])))
    {
        return Vec::new();
    }
    let value: Value =
        serde_json::from_str(&output("npm", &["list", "--global", "--depth=0", "--json"]))
            .unwrap_or(Value::Null);
    let matching: Vec<_> = value
        .get("dependencies")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(name, _)| matcher.relevant(&[name]))
        .collect();
    if matching.is_empty() {
        return Vec::new();
    }
    matching
        .into_iter()
        .map(|(name, metadata)| {
            let mut item = Match::new(Backend::Npm, name, name);
            item.version = metadata
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            item.origin = metadata
                .get("resolved")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            item.scope = if path_within(&prefix, &home()) {
                "user"
            } else {
                "system"
            }
            .to_owned();
            item.role = Role::Explicit;
            item
        })
        .collect()
}

fn cargo_root() -> PathBuf {
    std::env::var_os("CARGO_INSTALL_ROOT")
        .or_else(|| std::env::var_os("CARGO_HOME"))
        .map_or_else(|| home().join(".cargo"), PathBuf::from)
}

fn detect_cargo(query: &str) -> Vec<Match> {
    if !exists("cargo") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let value: Value =
        serde_json::from_str(&read(&cargo_root().join(".crates2.json"))).unwrap_or(Value::Null);
    static EXPRESSION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(.+?)\s+(\S+)\s+\((.+)\)$").expect("valid Cargo expression")
    });
    let mut found = Vec::new();
    for (key, details) in value
        .get("installs")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let Some(capture) = EXPRESSION.captures(key) else {
            continue;
        };
        let binaries: Vec<&str> = details
            .get("bins")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !matcher.relevant(&[&capture[1]])
            && !binaries.iter().any(|binary| matcher.relevant(&[*binary]))
        {
            continue;
        }
        let mut item = Match::new(Backend::Cargo, &capture[1], &capture[1]);
        item.version = capture[2].to_owned();
        item.origin = capture[3].to_owned();
        item.scope = "user".to_owned();
        item.role = Role::Explicit;
        if binaries.contains(&query) {
            item.command_path = which(query);
        }
        found.push(item);
    }
    found
}

fn detect_nix(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let mut found = Vec::new();
    if exists("nix") {
        let value: Value = serde_json::from_str(&output("nix", &["profile", "list", "--json"]))
            .unwrap_or(Value::Null);
        let elements = value.get("elements").unwrap_or(&value);
        for (id, _) in elements.as_object().into_iter().flatten() {
            if matcher.relevant(&[id]) {
                let mut item = Match::new(Backend::Nix, id, id);
                item.scope = "user".to_owned();
                item.role = Role::Explicit;
                found.push(item);
            }
        }
    }
    if found.is_empty() && exists("nix-env") {
        for line in output("nix-env", &["--query", "--installed", "--out-path"]).lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let Some(package) = fields.first() else {
                continue;
            };
            let (name, version) = split_name_version(package);
            if matcher.relevant(&[&name]) {
                let mut item = Match::new(Backend::NixLegacy, &name, &name);
                item.version = version;
                item.origin = fields.get(1).copied().unwrap_or_default().to_owned();
                item.scope = "user".to_owned();
                item.role = Role::Explicit;
                found.push(item);
            }
        }
    }
    found
}

fn split_name_version(value: &str) -> (String, String) {
    static EXPRESSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+)-([0-9][^\s]*)$").expect("valid package expression"));
    EXPRESSION.captures(value).map_or_else(
        || (value.to_owned(), String::new()),
        |capture| (capture[1].to_owned(), capture[2].to_owned()),
    )
}

fn detect_guix(query: &str) -> Vec<Match> {
    if !exists("guix") {
        return Vec::new();
    }
    let matcher = QueryMatcher::new(query);
    let profile = std::env::var_os("GUIX_PROFILE")
        .map_or_else(|| home().join(".guix-profile"), PathBuf::from);
    let option = format!("--profile={}", profile.display());
    output("guix", &["package", &option, "--list-installed"])
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 2 || !matcher.relevant(&[fields[0]]) {
                return None;
            }
            let mut item = Match::new(Backend::Guix, fields[0], fields[0]);
            item.version = fields[1].to_owned();
            item.profile = profile.display().to_string();
            item.scope = "user".to_owned();
            item.role = Role::Explicit;
            Some(item)
        })
        .collect()
}

fn detect_conda(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let manager = if exists("conda") {
        "conda"
    } else if exists("micromamba") {
        "micromamba"
    } else {
        return Vec::new();
    };
    let environments: Value =
        serde_json::from_str(&output(manager, &["env", "list", "--json"])).unwrap_or(Value::Null);
    let mut found = Vec::new();
    for profile in environments
        .get("envs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(128)
    {
        let value: Value =
            serde_json::from_str(&output(manager, &["list", "--json", "--prefix", profile]))
                .unwrap_or(Value::Null);
        for package in value.as_array().into_iter().flatten() {
            let Some(id) = package.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !matcher.relevant(&[id]) {
                continue;
            }
            let mut item = Match::new(
                if manager == "conda" {
                    Backend::Conda
                } else {
                    Backend::Micromamba
                },
                id,
                id,
            );
            item.version = package
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            item.origin = package
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            item.profile = profile.to_owned();
            item.scope = "user".to_owned();
            item.role = Role::Explicit;
            found.push(item);
        }
    }
    found
}

fn is_appimage(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 11];
    file.read_exact(&mut header).is_ok()
        && &header[..4] == b"\x7fELF"
        && &header[8..10] == b"AI"
        && matches!(header[10], 1 | 2)
}

fn safe_standalone(path: &Path) -> bool {
    path_within(path, &home())
        || path_within(path, Path::new("/usr/local"))
        || path_within(path, Path::new("/opt"))
}

fn find_executable(query: &str) -> Option<PathBuf> {
    which(query).or_else(|| {
        if query == query.to_lowercase() || query.contains('/') {
            return None;
        }
        std::env::var_os("PATH").and_then(|path| {
            let candidates: Vec<PathBuf> = std::env::split_paths(&path)
                .filter_map(|directory| fs::read_dir(directory).ok())
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|candidate| {
                    candidate
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(query))
                        && candidate.metadata().is_ok_and(|metadata| {
                            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                        })
                })
                .collect();
            (candidates.len() == 1).then(|| candidates[0].clone())
        })
    })
}

fn detect_owner(query: &str) -> Vec<Match> {
    static CACHE: OnceLock<OwnerCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let entry = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(query.to_owned())
        .or_insert_with(|| Arc::new(OnceLock::new()))
        .clone();
    entry.get_or_init(|| detect_owner_uncached(query)).clone()
}

fn detect_owner_uncached(query: &str) -> Vec<Match> {
    if query.is_empty()
        || Path::new(query)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(query)
    {
        return Vec::new();
    }
    let Some(visible) = find_executable(query) else {
        return Vec::new();
    };
    let visible = absolute_path(&visible);
    let resolved = fs::canonicalize(&visible).unwrap_or_else(|_| visible.clone());
    let resolved_text = resolved.display().to_string();
    let visible_text = visible.display().to_string();
    if let Some(kind) = rpm_manager() {
        let backend = Backend::parse(kind).expect("known RPM backend");
        for candidate in [&resolved_text, &visible_text] {
            let text = output(
                "rpm",
                &[
                    "-qf",
                    "--qf",
                    "%{NAME}\t%{VERSION}-%{RELEASE}\t%{ARCH}\t%{SUMMARY}\t%{SIZE}\n",
                    "--",
                    candidate,
                ],
            );
            let fields: Vec<&str> = text.trim().split('\t').collect();
            if fields.len() >= 2 && !fields[0].is_empty() {
                if kind == "RPM-OSTree"
                    && rpm_layered().is_some_and(|items| !items.contains(fields[0]))
                {
                    return Vec::new();
                }
                let architecture = fields.get(2).copied().unwrap_or_default();
                let id = if architecture.is_empty() {
                    fields[0].to_owned()
                } else {
                    format!("{}.{}", fields[0], architecture)
                };
                let mut item = Match::new(backend, id, fields[0]);
                item.version = fields[1].to_owned();
                item.architecture = architecture.to_owned();
                item.summary = fields.get(3).copied().unwrap_or_default().to_owned();
                item.installed_size_bytes = fields.get(4).and_then(|size| size.parse().ok());
                item.command_path = Some(visible.clone());
                return vec![item];
            }
        }
    }
    if exists("dpkg-query") {
        for candidate in [&resolved_text, &visible_text] {
            for line in output("dpkg-query", &["-S", candidate]).lines() {
                let Some((owner, _)) = line.rsplit_once(": ") else {
                    continue;
                };
                let details = output(
                    "dpkg-query",
                    &[
                        "-W",
                        "-f=${db:Status-Abbrev}\t${binary:Package}\t${Version}\t${binary:Summary}\t${Installed-Size}\n",
                        owner,
                    ],
                );
                let fields: Vec<&str> = details.trim().split('\t').collect();
                if fields.len() >= 3 && fields[0].as_bytes().get(1) == Some(&b'i') {
                    let mut item = Match::new(Backend::Apt, fields[1], fields[1]);
                    item.version = fields[2].to_owned();
                    item.summary = fields.get(3).copied().unwrap_or_default().to_owned();
                    item.installed_size_bytes = fields
                        .get(4)
                        .and_then(|size| size.parse::<u64>().ok())
                        .map(|size| size * 1024);
                    item.command_path = Some(visible.clone());
                    return vec![item];
                }
            }
        }
    }
    if exists("pacman") {
        for candidate in [&resolved_text, &visible_text] {
            let owner = output("pacman", &["-Qoq", candidate]);
            if let Some(name) = owner.lines().next() {
                let details = parse_key_value_records(&output("pacman", &["-Qi", name]));
                if let Some(record) = details.first() {
                    let id = record.get("Name").map_or(name, String::as_str);
                    let mut item = Match::new(Backend::Pacman, id, id);
                    item.version = record.get("Version").cloned().unwrap_or_default();
                    item.summary = record.get("Description").cloned().unwrap_or_default();
                    item.installed_size_bytes = record
                        .get("Installed Size")
                        .and_then(|size| parse_size(size));
                    item.role = if record
                        .get("Install Reason")
                        .is_some_and(|reason| reason.to_ascii_lowercase().contains("dependency"))
                    {
                        Role::Dependency
                    } else {
                        Role::Explicit
                    };
                    item.command_path = Some(visible.clone());
                    return vec![item];
                }
            }
        }
    }
    for (backend, program, args) in [
        (
            Backend::Apk,
            "apk",
            vec!["info", "--who-owns", &resolved_text],
        ),
        (Backend::Opkg, "opkg", vec!["search", &resolved_text]),
        (
            Backend::Xbps,
            "xbps-query",
            vec!["--ownedby", &resolved_text],
        ),
        (Backend::Eopkg, "eopkg", vec!["search-file", &resolved_text]),
    ] {
        if !exists(program) {
            continue;
        }
        let text = output(program, &args);
        let inventories: &[PackageRecord] = match backend {
            Backend::Apk => apk_inventory(),
            Backend::Opkg => opkg_inventory(),
            Backend::Xbps => xbps_inventory(),
            Backend::Eopkg => eopkg_inventory(),
            _ => unreachable!("ownership backend"),
        };
        if let Some(record) = inventories
            .iter()
            .find(|record| text.contains(&record.name))
        {
            let mut item = Match::new(backend, &record.name, &record.name);
            item.version.clone_from(&record.version);
            item.summary.clone_from(&record.summary);
            item.installed_size_bytes = record.size;
            item.command_path = Some(visible.clone());
            item.role = record.role;
            item.origin.clone_from(&record.origin);
            return vec![item];
        }
    }
    if exists("emerge") {
        let prefix = std::env::var_os("EPREFIX").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let root = prefix.join("var/db/pkg");
        for contents in WalkDir::new(&root)
            .min_depth(3)
            .max_depth(3)
            .into_iter()
            .flatten()
            .filter(|entry| entry.file_name() == "CONTENTS")
        {
            let owned = read(contents.path()).lines().any(|line| {
                line.split_whitespace()
                    .nth(1)
                    .is_some_and(|path| path == resolved_text || path == visible_text)
            });
            if !owned {
                continue;
            }
            let Some(package) = contents.path().parent() else {
                continue;
            };
            let Some(category) = package
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
            else {
                continue;
            };
            let package_dir = package
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let (name, version) = split_name_version(package_dir);
            let mut item = Match::new(Backend::Portage, format!("{category}/{name}"), name);
            item.version = version;
            item.command_path = Some(visible.clone());
            item.origin = read(&package.join("repository")).trim().to_owned();
            item.installed_size_bytes = read(&package.join("SIZE")).trim().parse().ok();
            item.role =
                if portage_world_contains(&read(&prefix.join("var/lib/portage/world")), &item.id) {
                    Role::Explicit
                } else {
                    Role::Dependency
                };
            item.scope = if path_within(&prefix, &home()) {
                "user"
            } else {
                "system"
            }
            .to_owned();
            return vec![item];
        }
    }
    if exists("removepkg") {
        if let Ok(entries) = fs::read_dir("/var/log/packages") {
            for entry in entries.flatten() {
                if !read(&entry.path()).lines().any(|line| {
                    line.trim_start_matches('.').trim_start_matches('/')
                        == resolved_text.trim_start_matches('/')
                        || line.trim_start_matches('.').trim_start_matches('/')
                            == visible_text.trim_start_matches('/')
                }) {
                    continue;
                }
                let package = entry.file_name().to_string_lossy().into_owned();
                let fields: Vec<&str> = package.rsplitn(4, '-').collect();
                let name = fields.get(3).copied().unwrap_or(&package);
                let mut item = Match::new(Backend::Slackware, name, name);
                item.version = fields.get(2).copied().unwrap_or_default().to_owned();
                item.command_path = Some(visible.clone());
                return vec![item];
            }
        }
    }
    if resolved_text.contains("/node_modules/") {
        let tail = resolved_text
            .split("/node_modules/")
            .nth(1)
            .unwrap_or_default();
        let pieces: Vec<&str> = tail.split('/').collect();
        let package =
            if pieces.first().is_some_and(|part| part.starts_with('@')) && pieces.len() > 1 {
                format!("{}/{}", pieces[0], pieces[1])
            } else {
                pieces.first().copied().unwrap_or_default().to_owned()
            };
        if let Some(mut item) = detect_npm(&package)
            .into_iter()
            .find(|item| item.id == package)
        {
            item.command_path = Some(visible.clone());
            return vec![item];
        }
    }
    if exists("brew") {
        let formula = output("brew", &["which-formula", &resolved_text]);
        for name in formula
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            if let Some(mut item) = detect_homebrew(name)
                .into_iter()
                .find(|item| item.id == name)
            {
                item.command_path = Some(visible.clone());
                return vec![item];
            }
        }
    }
    if is_appimage(&resolved) && safe_standalone(&resolved) {
        let name = resolved
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(query);
        let mut item = Match::new(Backend::AppImage, resolved_text, name);
        item.source_path = Some(resolved.clone());
        item.command_path = (visible != resolved).then_some(visible);
        item.scope = if path_within(&resolved, &home()) {
            "user"
        } else {
            "system"
        }
        .to_owned();
        item.role = Role::Explicit;
        item.installed_size_bytes = resolved.metadata().ok().map(|metadata| metadata.len());
        return vec![item];
    }
    let protected = [
        "/nix/store/",
        "/node_modules/",
        "/pipx/venvs/",
        "/flatpak/",
        "/snap/",
        "/Cellar/",
    ];
    if safe_standalone(&visible)
        && !protected
            .iter()
            .any(|marker| resolved_text.contains(marker))
    {
        let mut item = Match::new(Backend::Standalone, &visible_text, query);
        item.source_path = Some(visible.clone());
        item.command_path = Some(visible.clone());
        item.scope = if path_within(item.source_path.as_ref().expect("path"), &home()) {
            "user"
        } else {
            "system"
        }
        .to_owned();
        item.role = Role::Explicit;
        item.installed_size_bytes = visible.metadata().ok().map(|metadata| metadata.len());
        return vec![item];
    }
    Vec::new()
}

fn detect_appimages(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let direct = PathBuf::from(query);
    if direct.is_file()
        && (direct
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("AppImage"))
            || is_appimage(&direct))
    {
        let path = absolute_path(&direct);
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(query);
        let mut item = Match::new(Backend::AppImage, path.display().to_string(), name);
        item.source_path = Some(path.clone());
        item.scope = if path_within(&path, &home()) {
            "user"
        } else {
            "system"
        }
        .to_owned();
        item.role = Role::Explicit;
        item.installed_size_bytes = path.metadata().ok().map(|metadata| metadata.len());
        return vec![item];
    }
    let roots = [
        home().join("Applications"),
        home().join(".local/bin"),
        home().join("Downloads"),
        home().join("Desktop"),
        PathBuf::from("/opt"),
        PathBuf::from("/usr/local/bin"),
    ];
    let mut found = Vec::new();
    for root in roots.into_iter().filter(|path| path.is_dir()) {
        for entry in WalkDir::new(root)
            .max_depth(3)
            .into_iter()
            .flatten()
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !name.to_ascii_lowercase().ends_with(".appimage") || !matcher.relevant(&[name]) {
                continue;
            }
            let mut item = Match::new(
                Backend::AppImage,
                path.display().to_string(),
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(name),
            );
            item.source_path = Some(path.to_path_buf());
            item.scope = if path_within(path, &home()) {
                "user"
            } else {
                "system"
            }
            .to_owned();
            item.role = Role::Explicit;
            item.installed_size_bytes = path.metadata().ok().map(|metadata| metadata.len());
            found.push(item);
        }
    }
    found
}

fn desktop_entries() -> Vec<(String, String, String, PathBuf)> {
    let mut roots = vec![
        home().join(".local/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ];
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        roots.insert(0, PathBuf::from(data_home).join("applications"));
    }
    let mut entries = Vec::new();
    for root in roots.into_iter().filter(|path| path.is_dir()) {
        let Ok(files) = fs::read_dir(root) else {
            continue;
        };
        for file in files.flatten().filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "desktop")
        }) {
            let mut name = String::new();
            let mut executable = String::new();
            let mut container = String::new();
            for line in read(&file.path()).lines() {
                if let Some(value) = line.strip_prefix("Name=") {
                    if name.is_empty() {
                        name = value.to_owned();
                    }
                } else if let Some(value) = line.strip_prefix("Exec=") {
                    if executable.is_empty() {
                        executable = value
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .trim_matches(['\'', '"'])
                            .to_owned();
                    }
                } else if let Some(value) = line
                    .strip_prefix("X-Distrobox-Container=")
                    .or_else(|| line.strip_prefix("X-Toolbx-Container="))
                {
                    container = value.to_owned();
                }
            }
            if !name.is_empty() {
                entries.push((name, executable, container, file.path()));
            }
        }
    }
    entries
}

fn detect_desktop(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let mut found = Vec::new();
    for (name, executable, container, desktop) in desktop_entries() {
        if !matcher.relevant(&[
            &name,
            &executable,
            desktop
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        ]) {
            continue;
        }
        if !container.is_empty() {
            let mut item = Match::new(
                Backend::ContainerExport,
                desktop.display().to_string(),
                name,
            );
            item.profile = container;
            item.source_path = Some(desktop);
            item.role = Role::Explicit;
            found.push(item);
        } else {
            let path = PathBuf::from(&executable);
            if path.is_file() && is_appimage(&path) {
                let mut item = Match::new(Backend::AppImage, path.display().to_string(), name);
                item.source_path = Some(path);
                item.role = Role::Explicit;
                item.installed_size_bytes = item
                    .source_path
                    .as_ref()
                    .and_then(|source| source.metadata().ok())
                    .map(|metadata| metadata.len());
                found.push(item);
            } else if let Some(command) = Path::new(&executable)
                .file_name()
                .and_then(|value| value.to_str())
            {
                if [
                    "env",
                    "flatpak",
                    "snap",
                    "sh",
                    "bash",
                    "distrobox",
                    "toolbox",
                ]
                .contains(&command)
                {
                    continue;
                }
                for mut item in detect_owner(command) {
                    if item.backend != Backend::Standalone {
                        item.name.clone_from(&name);
                        found.push(item);
                    }
                }
            }
        }
    }
    found
}

fn detect_archive(query: &str) -> Vec<Match> {
    let path = PathBuf::from(query);
    if !path.is_file() {
        return Vec::new();
    }
    let lower = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower.ends_with(".rpm") && exists("rpm") {
        let path_text = path.display().to_string();
        let metadata = output(
            "rpm",
            &[
                "-qp",
                "--qf",
                "%{NAME}\t%{VERSION}-%{RELEASE}\t%{ARCH}\n",
                "--",
                &path_text,
            ],
        );
        let fields: Vec<&str> = metadata.trim().split('\t').collect();
        if fields.len() == 3 {
            let id = format!("{}.{}", fields[0], fields[2]);
            let installed = output("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}\n", "--", &id]);
            if !installed.trim().is_empty() {
                let backend =
                    Backend::parse(rpm_manager().unwrap_or("RPM")).expect("known RPM backend");
                let mut item = Match::new(backend, &id, fields[0]);
                item.version = installed.lines().next().unwrap_or_default().to_owned();
                item.architecture = fields[2].to_owned();
                item.source_path = Some(absolute_path(&path));
                item.evidence = format!("local RPM archive version {}", fields[1]);
                return vec![item];
            }
        }
    }
    if lower.ends_with(".deb") && exists("dpkg-deb") && exists("dpkg-query") {
        let path_text = path.display().to_string();
        let metadata = output(
            "dpkg-deb",
            &["-f", &path_text, "Package", "Version", "Architecture"],
        );
        let fields: Vec<&str> = metadata.lines().collect();
        if fields.len() >= 3 {
            let id = if fields[2] == "all" {
                fields[0].to_owned()
            } else {
                format!("{}:{}", fields[0], fields[2])
            };
            let installed = output("dpkg-query", &["-W", "-f=${Version}\n", &id]);
            if !installed.trim().is_empty() {
                let mut item = Match::new(Backend::Apt, id, fields[0]);
                item.version = installed.trim().to_owned();
                item.architecture = fields[2].to_owned();
                item.source_path = Some(absolute_path(&path));
                item.evidence = format!("local DEB archive version {}", fields[1]);
                return vec![item];
            }
        }
    }
    if lower.contains(".pkg.tar.") && exists("pacman") {
        let path_text = path.display().to_string();
        let archive = output("pacman", &["-Qp", &path_text]);
        let fields: Vec<&str> = archive.split_whitespace().collect();
        if fields.len() >= 2 {
            let installed = output("pacman", &["-Q", fields[0]]);
            let installed_fields: Vec<&str> = installed.split_whitespace().collect();
            if installed_fields.len() >= 2 && installed_fields[0] == fields[0] {
                let mut item = Match::new(Backend::Pacman, fields[0], fields[0]);
                item.version = installed_fields[1].to_owned();
                item.source_path = Some(absolute_path(&path));
                item.evidence = format!("local Arch package archive version {}", fields[1]);
                return vec![item];
            }
        }
    }
    if lower.ends_with(".apk") && exists("apk") && exists("tar") {
        let path_text = path.display().to_string();
        let metadata = output("tar", &["-xOf", &path_text, ".PKGINFO"]);
        let fields: HashMap<&str, &str> = metadata
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .collect();
        if let Some(name) = fields.get("pkgname") {
            if let Some(installed) = apk_inventory().iter().find(|record| record.name == *name) {
                let mut item = Match::new(Backend::Apk, *name, *name);
                item.version.clone_from(&installed.version);
                item.architecture = fields.get("arch").copied().unwrap_or_default().to_owned();
                item.source_path = Some(absolute_path(&path));
                item.evidence = format!(
                    "local APK archive version {}",
                    fields.get("pkgver").copied().unwrap_or("unknown")
                );
                return vec![item];
            }
        }
    }
    if (lower.ends_with(".ipk") || lower.ends_with(".opk")) && exists("opkg") {
        let control = read_ar_control(&path);
        let records = parse_key_value_records(&control);
        if let Some(fields) = records.first() {
            if let Some(name) = fields.get("Package") {
                if let Some(installed) = opkg_inventory().iter().find(|record| record.name == *name)
                {
                    let mut item = Match::new(Backend::Opkg, name, name);
                    item.version.clone_from(&installed.version);
                    item.architecture = fields.get("Architecture").cloned().unwrap_or_default();
                    item.source_path = Some(absolute_path(&path));
                    item.evidence = format!(
                        "local OPKG archive version {}",
                        fields.get("Version").map_or("unknown", String::as_str)
                    );
                    return vec![item];
                }
            }
        }
    }
    if lower.ends_with(".xbps") && exists("xbps-query") {
        let stem = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .trim_end_matches(".xbps");
        let package_version = stem.rsplit_once('.').map_or(stem, |(value, _)| value);
        let (name, archive_version) = split_xbps(package_version);
        if let Some(installed) = xbps_inventory().iter().find(|record| record.name == name) {
            let mut item = Match::new(Backend::Xbps, &name, &name);
            item.version.clone_from(&installed.version);
            item.source_path = Some(absolute_path(&path));
            item.evidence = format!("local XBPS archive version {archive_version}");
            return vec![item];
        }
    }
    if (lower.ends_with(".txz") || lower.ends_with(".tgz")) && exists("removepkg") {
        let stem = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let fields: Vec<&str> = stem.rsplitn(4, '-').collect();
        if fields.len() == 4 {
            let name = fields[3];
            if let Some(installed) = detect_slackware(name)
                .into_iter()
                .find(|record| record.name == name)
            {
                let mut item = Match::new(Backend::Slackware, name, name);
                item.version = installed.version;
                item.architecture = fields[1].to_owned();
                item.source_path = Some(absolute_path(&path));
                item.evidence = format!("local Slackware archive version {}", fields[2]);
                return vec![item];
            }
        }
    }
    if lower.ends_with(".eopkg") && exists("eopkg") {
        let path_text = path.display().to_string();
        let metadata = output("eopkg", &["info", &path_text]);
        let fields = parse_key_value_records(&metadata);
        if let Some(details) = fields.first() {
            if let Some(name) = details.get("Name") {
                if let Some(installed) = detect_eopkg(name)
                    .into_iter()
                    .find(|record| record.name == *name)
                {
                    let mut item = Match::new(Backend::Eopkg, name, name);
                    item.version = installed.version;
                    item.source_path = Some(absolute_path(&path));
                    item.evidence = format!(
                        "local Eopkg archive version {}",
                        details.get("Version").map_or("unknown", String::as_str)
                    );
                    return vec![item];
                }
            }
        }
    }
    if (lower.ends_with(".flatpak") || lower.ends_with(".flatpakref")) && exists("flatpak") {
        let id = if lower.ends_with(".flatpakref") {
            read(&path)
                .lines()
                .find_map(|line| line.strip_prefix("Name="))
                .unwrap_or_default()
                .trim()
                .to_owned()
        } else {
            let path_text = path.display().to_string();
            let reference = output("flatpak", &["info", "--show-ref", &path_text]);
            let parts: Vec<&str> = reference.trim().split('/').collect();
            if parts.first() == Some(&"app") && parts.len() >= 4 {
                parts[1].to_owned()
            } else {
                String::new()
            }
        };
        if !id.is_empty() {
            if let Some(mut item) = detect_flatpak(&id).into_iter().find(|item| item.id == id) {
                item.source_path = Some(absolute_path(&path));
                item.evidence = "matching local Flatpak bundle or reference".to_owned();
                return vec![item];
            }
        }
    }
    Vec::new()
}

fn read_ar_control(path: &Path) -> String {
    let Ok(payload) = fs::read(path) else {
        return String::new();
    };
    if !payload.starts_with(b"!<arch>\n") {
        return String::new();
    }
    let mut offset = 8_usize;
    while offset.saturating_add(60) <= payload.len() {
        let header = &payload[offset..offset + 60];
        let name = String::from_utf8_lossy(&header[..16])
            .trim()
            .trim_end_matches('/')
            .to_owned();
        let Ok(size) = String::from_utf8_lossy(&header[48..58])
            .trim()
            .parse::<usize>()
        else {
            return String::new();
        };
        let start = offset + 60;
        let end = start.saturating_add(size);
        if end > payload.len() {
            return String::new();
        }
        if name.starts_with("control.tar") {
            let data = &payload[start..end];
            let reader: Box<dyn Read> = if data.starts_with(&[0x1f, 0x8b]) {
                Box::new(flate2::read::GzDecoder::new(data))
            } else {
                Box::new(data)
            };
            let mut archive = tar::Archive::new(reader);
            let Ok(entries) = archive.entries() else {
                return String::new();
            };
            for entry in entries.flatten() {
                let name_matches = entry.path().ok().is_some_and(|entry_path| {
                    entry_path.to_string_lossy().trim_start_matches("./") == "control"
                });
                if name_matches {
                    let mut entry = entry;
                    let mut text = String::new();
                    let _ = entry.read_to_string(&mut text);
                    return text;
                }
            }
            return String::new();
        }
        offset = end.saturating_add(size % 2);
    }
    String::new()
}

#[derive(Default)]
struct AppStreamRecord {
    id: String,
    name: String,
    summary: String,
    package: String,
    binaries: Vec<String>,
}

fn appstream_bytes(path: &Path) -> Option<Vec<u8>> {
    const LIMIT: u64 = 32 * 1024 * 1024;
    let file = fs::File::open(path).ok()?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|value| value == "gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut bytes = Vec::new();
    reader.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= LIMIT).then_some(bytes)
}

fn appstream_record_relevant(record: &AppStreamRecord, matcher: &QueryMatcher) -> bool {
    matcher.relevant(&[
        &record.id,
        &record.name,
        &record.summary,
        &record.package,
        &record.binaries.join(" "),
    ])
}

fn parse_appstream_filtered(bytes: &[u8], matcher: Option<&QueryMatcher>) -> Vec<AppStreamRecord> {
    #[derive(Clone, Copy)]
    enum Field {
        None,
        Id,
        Name,
        Summary,
        Package,
        Binary,
    }

    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut current: Option<AppStreamRecord> = None;
    let mut field = Field::None;
    let mut records = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                field = match event.local_name().as_ref() {
                    b"component" => {
                        current = Some(AppStreamRecord::default());
                        Field::None
                    }
                    b"id" => Field::Id,
                    b"name" => Field::Name,
                    b"summary" => Field::Summary,
                    b"pkgname" => Field::Package,
                    b"binary" => Field::Binary,
                    _ => Field::None,
                };
            }
            Ok(Event::Text(text)) => {
                let Some(record) = &mut current else {
                    buffer.clear();
                    continue;
                };
                if matches!(field, Field::None) {
                    buffer.clear();
                    continue;
                }
                let value = reader
                    .decoder()
                    .decode(text.as_ref())
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default();
                if value.is_empty() {
                    buffer.clear();
                    continue;
                }
                match field {
                    Field::Id if record.id.is_empty() => record.id = value,
                    Field::Name if record.name.is_empty() => record.name = value,
                    Field::Summary if record.summary.is_empty() => record.summary = value,
                    Field::Package if record.package.is_empty() => record.package = value,
                    Field::Binary => record.binaries.push(value),
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                if event.local_name().as_ref() == b"component" {
                    if let Some(record) = current.take().filter(|record| {
                        (!record.id.is_empty()
                            || !record.package.is_empty()
                            || !record.binaries.is_empty())
                            && matcher
                                .is_none_or(|matcher| appstream_record_relevant(record, matcher))
                    }) {
                        records.push(record);
                    }
                }
                field = Field::None;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    records
}

#[cfg(test)]
fn parse_appstream(bytes: &[u8]) -> Vec<AppStreamRecord> {
    parse_appstream_filtered(bytes, None)
}

fn native_package_matches(package: &str) -> Vec<Match> {
    let base = package_base_ref(package);
    let from_matches = |items: &[Match]| {
        items
            .iter()
            .filter(|item| package_base_ref(&item.id) == base)
            .cloned()
            .collect()
    };
    let from_records = |backend: Backend, items: &[PackageRecord]| {
        items
            .iter()
            .filter(|item| package_base_ref(&item.name) == base)
            .map(|item| package_record_match(backend, item))
            .collect()
    };
    match native_family() {
        NativeFamily::Apt => from_matches(apt_inventory()),
        NativeFamily::Rpm => from_matches(rpm_inventory()),
        NativeFamily::Pacman => from_matches(pacman_inventory()),
        NativeFamily::Apk => from_records(Backend::Apk, apk_inventory()),
        NativeFamily::Xbps => from_records(Backend::Xbps, xbps_inventory()),
        NativeFamily::Eopkg => from_records(Backend::Eopkg, eopkg_inventory()),
        _ => native_detector()(package)
            .into_iter()
            .filter(|item| package_base_ref(&item.id) == base)
            .collect(),
    }
}

fn detect_appstream(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let roots = [
        PathBuf::from("/usr/share/metainfo"),
        PathBuf::from("/usr/share/appdata"),
        PathBuf::from("/usr/share/app-info/xmls"),
        home().join(".local/share/metainfo"),
    ];
    let mut records = Vec::new();
    for path in roots
        .into_iter()
        .flat_map(|root| fs::read_dir(root).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.ends_with(".xml") || name.ends_with(".xml.gz")
        })
        .take(4096)
    {
        if let Some(bytes) = appstream_bytes(&path) {
            records.extend(parse_appstream_filtered(&bytes, Some(&matcher)));
        }
    }
    let mut found = Vec::new();
    for record in records {
        let mut owners = if record.package.is_empty() {
            Vec::new()
        } else {
            native_package_matches(&record.package)
        };
        if owners.is_empty() {
            owners = record
                .binaries
                .iter()
                .find_map(|binary| {
                    let command = Path::new(binary).file_name()?.to_str()?;
                    let owned = detect_owner(command);
                    (!owned.is_empty()).then_some(owned)
                })
                .unwrap_or_default();
        }
        for mut owner in owners {
            if !record.name.is_empty() {
                owner.name.clone_from(&record.name);
            }
            owner.evidence = format!("AppStream component {}", record.id);
            found.push(owner);
        }
    }
    found
}

type Detector = fn(&str) -> Vec<Match>;

fn native_detector() -> Detector {
    match native_family() {
        NativeFamily::Apt => detect_apt,
        NativeFamily::Rpm => detect_rpm,
        NativeFamily::Pacman => detect_pacman,
        NativeFamily::Apk => detect_apk,
        NativeFamily::Xbps => detect_xbps,
        NativeFamily::Portage => detect_portage,
        NativeFamily::Slackware => detect_slackware,
        NativeFamily::Eopkg => detect_eopkg,
        NativeFamily::Swupd => detect_swupd,
        NativeFamily::Unknown => |_| Vec::new(),
    }
}

fn detectors() -> Vec<Detector> {
    let mut selected: Vec<Detector> = vec![
        detect_flatpak,
        detect_snap,
        detect_homebrew,
        detect_gearlever,
        detect_pipx,
        detect_uv,
        detect_conda,
        detect_npm,
        detect_cargo,
        detect_nix,
        detect_guix,
        detect_swupd_third_party,
        detect_appimages,
        detect_desktop,
        detect_appstream,
    ];
    selected.push(native_detector());
    if exists("opkg") {
        selected.push(detect_opkg);
    }
    selected
}

fn exact_match(item: &Match, matcher: &QueryMatcher) -> bool {
    [item.id.as_str(), item.name.as_str()]
        .into_iter()
        .any(|value| matcher.exact(value))
}

fn score(item: &Match, matcher: &QueryMatcher) -> (u8, usize, String) {
    let needle = matcher.normalized();
    let id = norm(&item.id);
    let name = norm(&item.name);
    let command = item
        .command_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|value| norm(&value.to_string_lossy()))
        .unwrap_or_default();
    let tier = if command == needle {
        0
    } else if id == needle || name == needle {
        1
    } else if id.starts_with(needle) || name.starts_with(needle) {
        2
    } else {
        3
    };
    (tier, name.len().min(id.len()), name)
}

pub fn find_matches(query: &str) -> Vec<Match> {
    let matcher = QueryMatcher::new(query);
    let archive = detect_archive(query);
    if !archive.is_empty() {
        return archive;
    }
    let owner = detect_owner(query);
    let mut selected = detectors();
    if !owner.is_empty() {
        let direct_file = owner
            .iter()
            .all(|item| matches!(item.backend.as_str(), "Standalone" | "AppImage"));
        if direct_file {
            let appimage = owner.iter().any(|item| item.backend == Backend::AppImage);
            selected.retain(|detector| {
                std::ptr::fn_addr_eq(*detector, detect_gearlever as Detector)
                    || (appimage && std::ptr::fn_addr_eq(*detector, detect_desktop as Detector))
            });
        } else {
            let native = native_detector();
            selected.retain(|detector| {
                !std::ptr::fn_addr_eq(*detector, native)
                    && !std::ptr::fn_addr_eq(*detector, detect_opkg as Detector)
            });
        }
    }
    let discover = || {
        selected
            .par_iter()
            .flat_map_iter(|detector| detector(query))
            .collect()
    };
    let mut results: Vec<Match> = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .thread_name(|index| format!("uninstall-discovery-{index}"))
        .build()
        .map_or_else(|_| discover(), |pool| pool.install(discover));
    results.extend(owner);
    let mut unique = BTreeMap::new();
    for item in results {
        unique.entry(item.key()).or_insert(item);
    }
    let mut results: Vec<Match> = unique.into_values().collect();
    let gearlever_paths: HashSet<PathBuf> = results
        .iter()
        .filter(|item| item.backend == Backend::GearLever)
        .filter_map(|item| item.source_path.as_ref())
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect();
    if !gearlever_paths.is_empty() {
        results.retain(|item| {
            item.backend != Backend::AppImage
                || item.source_path.as_ref().is_none_or(|path| {
                    !gearlever_paths
                        .contains(&fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
                })
        });
    }
    let has_exact_command = results.iter().any(|item| {
        item.command_path
            .as_ref()
            .and_then(|path| path.file_name())
            .is_some_and(|name| matcher.exact(&name.to_string_lossy()))
    });
    if has_exact_command {
        results.retain(|item| {
            exact_match(item, &matcher)
                || item
                    .command_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .is_some_and(|name| matcher.exact(&name.to_string_lossy()))
        });
    }
    let command_matches: Vec<Match> = results
        .iter()
        .filter(|item| item.command_path.is_some())
        .cloned()
        .collect();
    if !command_matches.is_empty() {
        let managed: HashSet<PathBuf> = command_matches
            .iter()
            .filter(|item| item.backend != Backend::Standalone)
            .filter_map(|item| item.command_path.clone())
            .collect();
        results.retain(|item| {
            if item.backend == Backend::Standalone
                && item
                    .command_path
                    .as_ref()
                    .is_some_and(|path| managed.contains(path))
            {
                return false;
            }
            item.command_path.is_some() || exact_match(item, &matcher)
        });
    }
    results.sort_by_key(|item| score(item, &matcher));
    results
}

pub fn filter_dependencies(matches: Vec<Match>, query: &str, show: bool) -> (Vec<Match>, usize) {
    if show {
        return (matches, 0);
    }
    let matcher = QueryMatcher::new(query);
    let mut visible = Vec::new();
    let mut hidden = Vec::new();
    for item in matches {
        if item.role.is_dependency() && item.command_path.is_none() && !exact_match(&item, &matcher)
        {
            hidden.push(item);
        } else {
            visible.push(item);
        }
    }
    if visible.is_empty() {
        (hidden, 0)
    } else {
        let count = hidden.len();
        (visible, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xbps_name_version_parser_preserves_hyphens() {
        assert_eq!(
            split_xbps("my-tool-1.2_3"),
            ("my-tool".to_owned(), "1.2_3".to_owned())
        );
    }

    #[test]
    fn exact_identifier_ranks_before_fuzzy_name() {
        let exact = Match::new(Backend::Apt, "edit", "edit");
        let fuzzy = Match::new(Backend::Apt, "editor-libs", "editor-libs");
        let matcher = QueryMatcher::new("edit");
        assert!(score(&exact, &matcher) < score(&fuzzy, &matcher));
    }

    #[test]
    fn key_value_parser_keeps_continuation_lines() {
        let records =
            parse_key_value_records("Name : app\nRequired By : one\n              two\n\n");
        assert_eq!(records[0]["Required By"], "one two");
    }

    #[test]
    fn appstream_parser_reads_names_packages_and_binaries() {
        let records = parse_appstream(
            br#"<components><component type="desktop-application">
                <id>org.example.Editor</id><name>Example Editor</name>
                <summary>Edit files safely</summary><pkgname>example-editor</pkgname>
                <provides><binary>example-edit</binary></provides>
            </component></components>"#,
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "org.example.Editor");
        assert_eq!(records[0].name, "Example Editor");
        assert_eq!(records[0].package, "example-editor");
        assert_eq!(records[0].binaries, ["example-edit"]);
    }

    #[test]
    fn portage_world_handles_versions_slots_and_use_flags() {
        let world = "# comment\n@system\n>=app-editors/neovim-0.10:0[luajit]\n";
        assert!(portage_world_contains(world, "app-editors/neovim"));
        assert!(!portage_world_contains(world, "dev-libs/libtermkey"));
    }

    #[test]
    fn fuzzy_dependencies_are_hidden_when_an_app_is_visible() {
        let app = Match::new(Backend::Dnf, "editor", "Editor");
        let mut library = Match::new(Backend::Dnf, "editor-libs", "editor-libs");
        library.role = Role::Dependency;
        let (visible, hidden) = filter_dependencies(vec![app, library], "editor", false);
        assert_eq!(visible.len(), 1);
        assert_eq!(hidden, 1);
    }

    #[test]
    fn sole_dependency_result_is_not_hidden() {
        let mut library = Match::new(Backend::Dnf, "library-tools", "library-tools");
        library.role = Role::Dependency;
        let (visible, hidden) = filter_dependencies(vec![library], "library", false);
        assert_eq!(visible.len(), 1);
        assert_eq!(hidden, 0);
    }

    #[test]
    fn exact_dependency_is_never_hidden() {
        let mut library = Match::new(Backend::Dnf, "library", "library");
        library.role = Role::Dependency;
        let (visible, hidden) = filter_dependencies(vec![library], "library", false);
        assert_eq!(visible.len(), 1);
        assert_eq!(hidden, 0);
    }

    #[test]
    fn appimage_signature_is_detected_without_an_extension() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("app");
        let mut header = [0_u8; 11];
        header[..4].copy_from_slice(b"\x7fELF");
        header[8..10].copy_from_slice(b"AI");
        header[10] = 2;
        fs::write(&path, header).expect("write");
        assert!(is_appimage(&path));
    }

    #[test]
    fn ordinary_elf_header_is_not_an_appimage() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("app");
        fs::write(&path, b"\x7fELFordinary").expect("write");
        assert!(!is_appimage(&path));
    }
}
