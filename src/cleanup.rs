use crate::command::{exists, run};
use crate::model::{Backend, Match};
use crate::util::{absolute_path, home, norm, path_within};
use rustix::fd::OwnedFd;
use rustix::fs::{AtFlags, Mode, OFlags, open, openat, renameat, statat, unlinkat};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct CleanupCandidate {
    pub path: PathBuf,
    pub size_bytes: Option<u64>,
    parent: PathBuf,
    name: OsString,
    parent_fd: OwnedFd,
    device: u64,
    inode: u64,
    mode: u32,
    change_time_seconds: i64,
    change_time_nanos: u64,
    parent_device: u64,
    parent_inode: u64,
}

#[derive(Debug, Clone)]
pub struct DataOption {
    pub label: String,
    pub path: Option<PathBuf>,
    pub backend: Option<Backend>,
    pub size_bytes: Option<u64>,
}

pub fn manager_cleanup_supported(item: &Match) -> bool {
    matches!(
        item.backend.as_str(),
        "Flatpak" | "Snap" | "APT" | "Homebrew Cask" | "Eopkg"
    )
}

pub fn manager_cleanup_label(item: &Match) -> Option<String> {
    match item.backend.as_str() {
        "Flatpak" => Some("Sandbox data and permissions".to_owned()),
        "Snap" => Some("Snap data and retained removal snapshot".to_owned()),
        "APT" => Some("System configuration files (APT purge)".to_owned()),
        "Homebrew Cask" => Some("Cask-associated files (Homebrew zap)".to_owned()),
        "Eopkg" => Some("System configuration files (Eopkg purge)".to_owned()),
        _ => None,
    }
}

fn flatpak_data_path(item: &Match) -> Option<PathBuf> {
    (item.backend == Backend::Flatpak).then(|| home().join(".var/app").join(&item.id))
}

pub fn manager_cleanup_size(item: &Match) -> Option<u64> {
    match item.backend.as_str() {
        "Flatpak" => flatpak_data_path(item).map(|path| path_size(&path).unwrap_or(0)),
        "Snap" => Some(path_size(&home().join("snap").join(&item.id)).unwrap_or(0)),
        _ => None,
    }
}

fn xdg_dir(variable: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or(fallback)
}

pub fn data_roots() -> Vec<PathBuf> {
    let user_home = home();
    let mut roots = vec![
        xdg_dir("XDG_CACHE_HOME", user_home.join(".cache")),
        xdg_dir("XDG_CONFIG_HOME", user_home.join(".config")),
        xdg_dir("XDG_DATA_HOME", user_home.join(".local/share")),
        xdg_dir("XDG_STATE_HOME", user_home.join(".local/state")),
    ];
    roots.sort();
    roots.dedup();
    roots
}

fn data_keys(items: &[Match]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for item in items {
        for value in [&item.name, &item.id] {
            let simple = value
                .rsplit('.')
                .next()
                .unwrap_or(value)
                .split(':')
                .next()
                .unwrap_or(value);
            for candidate in [value.as_str(), simple] {
                let key = norm(candidate);
                if key.len() >= 3 {
                    keys.insert(key);
                }
            }
        }
        if let Some(path) = &item.command_path {
            if let Some(stem) = path.file_stem().and_then(OsStr::to_str) {
                let key = norm(stem);
                if key.len() >= 3 {
                    keys.insert(key);
                }
            }
        }
    }
    keys
}

fn safe_root(path: &Path) -> bool {
    let absolute = absolute_path(path);
    let user_home = home();
    absolute.is_absolute()
        && path_within(&absolute, &user_home)
        && absolute != user_home
        && absolute != Path::new("/")
}

pub fn find_user_data(items: &[Match]) -> Vec<PathBuf> {
    let keys = data_keys(items);
    if keys.is_empty() {
        return Vec::new();
    }
    let manager_owned: BTreeSet<PathBuf> = items.iter().filter_map(flatpak_data_path).collect();
    let mut found = BTreeSet::new();
    for root in data_roots()
        .into_iter()
        .filter(|root| safe_root(root) && root.is_dir())
    {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if manager_owned.contains(&path) {
                continue;
            }
            let name = entry.file_name();
            let mut candidate = name.to_string_lossy().to_string();
            for suffix in [".desktop", ".conf", ".config", ".cache"] {
                if let Some(stripped) = candidate.strip_suffix(suffix) {
                    candidate = stripped.to_owned();
                    break;
                }
            }
            if keys.contains(&norm(&candidate)) {
                found.insert(path);
            }
        }
    }
    found.into_iter().collect()
}

pub fn data_options(items: &[Match], detected: &[PathBuf]) -> Vec<DataOption> {
    let mut options = Vec::new();
    let mut seen = BTreeSet::new();
    for item in items {
        if manager_cleanup_supported(item) && seen.insert(item.backend) {
            let backend_items: Vec<&Match> = items
                .iter()
                .filter(|candidate| candidate.backend == item.backend)
                .collect();
            let sizes: Vec<Option<u64>> = backend_items
                .iter()
                .map(|candidate| manager_cleanup_size(candidate))
                .collect();
            options.push(DataOption {
                label: format!(
                    "[{}] {}",
                    item.backend,
                    manager_cleanup_label(item).unwrap_or_default()
                ),
                path: None,
                backend: Some(item.backend),
                size_bytes: sizes
                    .iter()
                    .all(Option::is_some)
                    .then(|| sizes.into_iter().flatten().sum()),
            });
        }
    }
    options.extend(detected.iter().map(|path| DataOption {
        label: format!("[Detected] {}", path.display()),
        path: Some(path.clone()),
        backend: None,
        size_bytes: path_size(path),
    }));
    options
}

pub fn path_size(path: &Path) -> Option<u64> {
    if !path.exists() && !path.is_symlink() {
        return None;
    }
    if path.is_symlink() {
        return fs::symlink_metadata(path)
            .ok()
            .map(|metadata| metadata.len());
    }
    if exists("du") {
        let path_text = path.display().to_string();
        let result = run("du", ["-sk", "--", &path_text], Duration::from_secs(30));
        if result.ok() {
            if let Some(kib) = result
                .stdout
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
            {
                return Some(kib.saturating_mul(1024));
            }
        }
    }
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

pub fn snapshot(path: &Path) -> Result<CleanupCandidate, String> {
    let path = absolute_path(path);
    if !path.is_absolute() || path == Path::new("/") || path == home() {
        return Err(format!("refusing unsafe cleanup target {}", path.display()));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "cleanup target has no parent directory".to_owned())?
        .to_path_buf();
    let name = path
        .file_name()
        .ok_or_else(|| "cleanup target has no file name".to_owned())?
        .to_os_string();
    let parent_metadata = fs::metadata(&parent)
        .map_err(|error| format!("cannot inspect {}: {error}", parent.display()))?;
    let parent_fd = open(
        &parent,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("cannot hold {} open: {error}", parent.display()))?;
    let target_fd = openat(
        &parent_fd,
        &name,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("cannot hold {} open: {error}", path.display()))?;
    let stat = rustix::fs::fstat(&target_fd)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    Ok(CleanupCandidate {
        size_bytes: path_size(&path),
        path,
        parent,
        name,
        parent_fd,
        device: stat.st_dev,
        inode: stat.st_ino,
        mode: stat.st_mode,
        change_time_seconds: stat.st_ctime,
        change_time_nanos: stat.st_ctime_nsec as u64,
        parent_device: parent_metadata.dev(),
        parent_inode: parent_metadata.ino(),
    })
}

fn unchanged(candidate: &CleanupCandidate) -> Result<bool, String> {
    let parent = fs::metadata(&candidate.parent).map_err(|error| error.to_string())?;
    if parent.dev() != candidate.parent_device || parent.ino() != candidate.parent_inode {
        return Ok(false);
    }
    let stat = statat(
        &candidate.parent_fd,
        &candidate.name,
        AtFlags::SYMLINK_NOFOLLOW,
    )
    .map_err(|error| error.to_string())?;
    Ok(stat.st_dev == candidate.device
        && stat.st_ino == candidate.inode
        && stat.st_mode == candidate.mode
        && stat.st_ctime == candidate.change_time_seconds
        && stat.st_ctime_nsec as u64 == candidate.change_time_nanos)
}

fn tombstone_name() -> OsString {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    OsString::from(format!(
        ".uninstall-trash-{}-{timestamp:x}",
        std::process::id()
    ))
}

pub fn remove(candidate: CleanupCandidate) -> Result<(), String> {
    if !unchanged(&candidate)? {
        return Err(format!(
            "{} changed after it was selected",
            candidate.path.display()
        ));
    }
    let file_type = candidate.mode & libc::S_IFMT;
    if file_type == libc::S_IFDIR {
        let tombstone = tombstone_name();
        renameat(
            &candidate.parent_fd,
            &candidate.name,
            &candidate.parent_fd,
            &tombstone,
        )
        .map_err(|error| format!("could not isolate {}: {error}", candidate.path.display()))?;
        let tombstone_path = candidate.parent.join(&tombstone);
        fs::remove_dir_all(&tombstone_path)
            .map_err(|error| format!("could not delete {}: {error}", candidate.path.display()))
    } else {
        unlinkat(&candidate.parent_fd, &candidate.name, AtFlags::empty())
            .map_err(|error| format!("could not delete {}: {error}", candidate.path.display()))
    }
}

pub fn cleanup_command(path: &Path) -> Vec<String> {
    if path.is_dir() && !path.is_symlink() {
        vec![
            "rm".to_owned(),
            "-r".to_owned(),
            "--".to_owned(),
            path.display().to_string(),
        ]
    } else {
        vec!["rm".to_owned(), "--".to_owned(), path.display().to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn selected_file_is_removed_by_held_parent() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("example");
        fs::write(&path, "data").expect("write");
        let candidate = snapshot(&path).expect("snapshot");
        remove(candidate).expect("remove");
        assert!(!path.exists());
    }

    #[test]
    fn replacement_is_refused() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("example");
        fs::write(&path, "first").expect("write");
        let candidate = snapshot(&path).expect("snapshot");
        fs::remove_file(&path).expect("unlink");
        fs::write(&path, "second").expect("replace");
        assert!(remove(candidate).is_err());
        assert!(path.exists());
    }
}
