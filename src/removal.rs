use crate::command::{exists, which};
use crate::discovery::{dnf_binary, rpm_manager};
use crate::model::{Match, RemovalBatch};
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub fn privilege(command: Vec<String>) -> Result<Vec<String>, String> {
    if rustix::process::geteuid().is_root() {
        return Ok(command);
    }
    for helper in ["sudo", "doas", "pkexec"] {
        if exists(helper) {
            let mut elevated = vec![helper.to_owned()];
            if helper == "sudo" {
                elevated.push("--".to_owned());
            }
            elevated.extend(command);
            return Ok(elevated);
        }
    }
    Err(
        "this operation needs root privileges, but sudo, doas, and pkexec are unavailable"
            .to_owned(),
    )
}

fn system(command: Vec<String>) -> Result<Vec<String>, String> {
    privilege(command)
}

fn direct_file_command(command: Vec<String>, paths: &[&Path]) -> Result<Vec<String>, String> {
    let mut needs_privilege = false;
    for path in paths {
        let parent = path
            .parent()
            .ok_or_else(|| "a direct-removal target has no parent directory".to_owned())?;
        let writable = crate::command::run(
            "test",
            ["-w", &parent.display().to_string()],
            std::time::Duration::from_secs(2),
        )
        .ok();
        if writable {
            continue;
        }
        needs_privilege = true;
        let metadata = parent
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", parent.display()))?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(format!(
                "refusing privileged direct deletion through untrusted directory {}",
                parent.display()
            ));
        }
    }
    if needs_privilege {
        system(command)
    } else {
        Ok(command)
    }
}

fn flatpak_args(item: &Match, remove_data: bool) -> Vec<String> {
    let mut command = vec![
        "flatpak".to_owned(),
        "uninstall".to_owned(),
        "-y".to_owned(),
    ];
    if item.scope == "user" {
        command.push("--user".to_owned());
    } else if !item.installation.is_empty() && item.installation != "default" {
        command.push(format!("--installation={}", item.installation));
    } else {
        command.push("--system".to_owned());
    }
    if remove_data {
        command.push("--delete-data".to_owned());
    }
    command.push(item.id.clone());
    command
}

pub fn command_for(item: &Match, manager_cleanup: bool) -> Result<Vec<String>, String> {
    let id = item.id.clone();
    match item.backend.as_str() {
        "Flatpak" => Ok(flatpak_args(item, manager_cleanup)),
        "Snap" => {
            let mut command = vec!["snap".to_owned(), "remove".to_owned()];
            if manager_cleanup {
                command.push("--purge".to_owned());
            }
            command.push(id);
            system(command)
        }
        "APT" => system(vec![
            "apt-get".to_owned(),
            if manager_cleanup { "purge" } else { "remove" }.to_owned(),
            id,
        ]),
        "APT-RPM" => system(vec!["apt-get".to_owned(), "remove".to_owned(), id]),
        "DNF" => system(vec![
            dnf_binary().unwrap_or("dnf").to_owned(),
            "remove".to_owned(),
            id,
        ]),
        "YUM" => system(vec!["yum".to_owned(), "remove".to_owned(), id]),
        "RPM" => system(vec!["rpm".to_owned(), "-e".to_owned(), id]),
        "RPM-OSTree" => system(vec!["rpm-ostree".to_owned(), "uninstall".to_owned(), id]),
        "Zypper" => {
            if Path::new("/run/transactional-update.conf").exists()
                && exists("transactional-update")
            {
                system(vec![
                    "transactional-update".to_owned(),
                    "--non-interactive".to_owned(),
                    "pkg".to_owned(),
                    "remove".to_owned(),
                    id,
                ])
            } else {
                system(vec![
                    "zypper".to_owned(),
                    "--non-interactive".to_owned(),
                    "remove".to_owned(),
                    id,
                ])
            }
        }
        "URPMI" => system(vec!["urpme".to_owned(), "--auto".to_owned(), id]),
        "Pacman" => system(vec![
            "pacman".to_owned(),
            "--noconfirm".to_owned(),
            "-R".to_owned(),
            id,
        ]),
        "APK" => system(vec!["apk".to_owned(), "del".to_owned(), id]),
        "OPKG" => system(vec!["opkg".to_owned(), "remove".to_owned(), id]),
        "XBPS" => system(vec![
            "xbps-remove".to_owned(),
            "--recursive".to_owned(),
            "--yes".to_owned(),
            id,
        ]),
        "Portage" => {
            let atom = if item.version.is_empty() {
                format!("={id}")
            } else {
                format!("={id}-{}", item.version)
            };
            let command = vec![
                "emerge".to_owned(),
                "--ask=n".to_owned(),
                "--verbose".to_owned(),
                "--depclean".to_owned(),
                atom,
            ];
            if item.scope == "user" {
                Ok(command)
            } else {
                system(command)
            }
        }
        "Slackware" => system(vec!["removepkg".to_owned(), id]),
        "Eopkg" => {
            let mut command = vec![
                "eopkg".to_owned(),
                "remove".to_owned(),
                "--yes-all".to_owned(),
            ];
            if manager_cleanup {
                command.push("--purge".to_owned());
            }
            command.push(id);
            system(command)
        }
        "Swupd" => system(vec!["swupd".to_owned(), "bundle-remove".to_owned(), id]),
        "Swupd 3rd-party" => system(vec![
            "swupd".to_owned(),
            "3rd-party".to_owned(),
            "bundle-remove".to_owned(),
            "--repo".to_owned(),
            item.origin.clone(),
            id,
        ]),
        "Homebrew" => Ok(vec!["brew".to_owned(), "uninstall".to_owned(), id]),
        "Homebrew Cask" => {
            let mut command = vec![
                "brew".to_owned(),
                "uninstall".to_owned(),
                "--cask".to_owned(),
            ];
            if manager_cleanup {
                command.push("--zap".to_owned());
            }
            command.push(id);
            Ok(command)
        }
        "Cargo" => Ok(vec!["cargo".to_owned(), "uninstall".to_owned(), id]),
        "Pipx" => {
            let mut command = vec!["pipx".to_owned(), "uninstall".to_owned()];
            if item.scope == "system" {
                command.push("--global".to_owned());
            }
            command.push(id);
            if item.scope == "system" {
                system(command)
            } else {
                Ok(command)
            }
        }
        "UV Tool" => Ok(vec![
            "uv".to_owned(),
            "tool".to_owned(),
            "uninstall".to_owned(),
            id,
        ]),
        "NPM" => {
            let command = vec![
                "npm".to_owned(),
                "uninstall".to_owned(),
                "--global".to_owned(),
                id,
            ];
            if item.scope == "system" {
                system(command)
            } else {
                Ok(command)
            }
        }
        "Nix" => {
            let mut command = vec!["nix".to_owned(), "profile".to_owned(), "remove".to_owned()];
            if !item.profile.is_empty() {
                command.extend(["--profile".to_owned(), item.profile.clone()]);
            }
            command.push(id);
            Ok(command)
        }
        "Nix Legacy" => Ok(vec!["nix-env".to_owned(), "--uninstall".to_owned(), id]),
        "Guix" => Ok(vec![
            "guix".to_owned(),
            "package".to_owned(),
            format!("--profile={}", item.profile),
            format!("--remove={id}"),
        ]),
        "Conda" | "Micromamba" => {
            let program = if item.backend == "Conda" {
                "conda"
            } else {
                "micromamba"
            };
            Ok(vec![
                program.to_owned(),
                "remove".to_owned(),
                "--yes".to_owned(),
                "--prefix".to_owned(),
                item.profile.clone(),
                id,
            ])
        }
        "Gear Lever" => {
            let mut command = if exists("gearlever") {
                vec!["gearlever".to_owned()]
            } else if exists("gearlever-cli") {
                vec!["gearlever-cli".to_owned()]
            } else if exists("flatpak") {
                vec![
                    "flatpak".to_owned(),
                    "run".to_owned(),
                    "it.mijorus.gearlever".to_owned(),
                ]
            } else {
                return Err("Gear Lever is no longer available".to_owned());
            };
            let path = item
                .source_path
                .as_ref()
                .ok_or_else(|| "Gear Lever did not report a managed path".to_owned())?;
            command.extend([
                "--remove".to_owned(),
                path.display().to_string(),
                "--yes".to_owned(),
            ]);
            Ok(command)
        }
        "AppImage" => {
            let path = item
                .source_path
                .as_ref()
                .ok_or_else(|| "the AppImage path is unavailable".to_owned())?;
            let mut command = vec!["rm".to_owned(), "--".to_owned(), path.display().to_string()];
            let mut paths = vec![path.as_path()];
            if let Some(exposed) = &item.command_path {
                if exposed != path
                    && std::fs::canonicalize(exposed).ok().as_ref()
                        == std::fs::canonicalize(path).ok().as_ref()
                {
                    command.push(exposed.display().to_string());
                    paths.push(exposed.as_path());
                }
            }
            direct_file_command(command, &paths)
        }
        "Standalone" => {
            let path = item
                .source_path
                .as_ref()
                .ok_or_else(|| "the executable path is unavailable".to_owned())?;
            let command = vec!["rm".to_owned(), "--".to_owned(), path.display().to_string()];
            direct_file_command(command, &[path])
        }
        "Container Export" => Err(
            "remove this application inside its container; the host export is not the application"
                .to_owned(),
        ),
        backend => Err(format!("{backend} removal is not supported safely")),
    }
}

fn batchable(backend: &str) -> bool {
    matches!(
        backend,
        "Flatpak"
            | "APT"
            | "APT-RPM"
            | "DNF"
            | "YUM"
            | "RPM"
            | "RPM-OSTree"
            | "Zypper"
            | "URPMI"
            | "Pacman"
            | "APK"
            | "OPKG"
            | "XBPS"
            | "Slackware"
            | "Eopkg"
            | "Homebrew"
            | "Homebrew Cask"
            | "NPM"
    )
}

pub fn build_batches(
    items: &[Match],
    cleanup_backends: &[String],
) -> Result<Vec<RemovalBatch>, String> {
    let mut grouped: BTreeMap<String, Vec<Match>> = BTreeMap::new();
    for item in items {
        let key = if batchable(&item.backend) {
            format!(
                "{}\0{}\0{}\0{}",
                item.backend, item.scope, item.installation, item.profile
            )
        } else {
            item.key()
        };
        grouped.entry(key).or_default().push(item.clone());
    }
    let mut batches = Vec::new();
    for group in grouped.into_values() {
        let cleanup = cleanup_backends
            .iter()
            .any(|backend| backend == &group[0].backend);
        let mut command = command_for(&group[0], cleanup)?;
        if group.len() > 1 {
            let first_id = &group[0].id;
            let position = command
                .iter()
                .rposition(|part| part == first_id)
                .ok_or_else(|| "could not construct a grouped removal command".to_owned())?;
            command.splice(
                position..=position,
                group.iter().map(|item| item.id.clone()),
            );
        }
        batches.push(RemovalBatch {
            items: group,
            command,
        });
    }
    Ok(batches)
}

pub fn pin_command(command: &[String]) -> Result<Vec<String>, String> {
    if command.is_empty() {
        return Err("empty removal command".to_owned());
    }
    let mut pinned = command.to_vec();
    let privileged = ["sudo", "doas", "pkexec"].contains(&pinned[0].as_str());
    let executable_index = if privileged {
        pinned
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, part)| part.as_str() != "--")
            .map(|(index, _)| index)
            .ok_or_else(|| "missing privileged command".to_owned())?
    } else {
        0
    };
    let indexes = if privileged {
        vec![0, executable_index]
    } else {
        vec![0]
    };
    for index in indexes {
        let located = which(&pinned[index])
            .ok_or_else(|| format!("{} is no longer available", pinned[index]))?;
        let path = std::fs::canonicalize(&located)
            .map_err(|error| format!("cannot resolve {}: {error}", located.display()))?;
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("cannot validate {}: {error}", path.display()))?;
        if privileged && (metadata.uid() != 0 || metadata.mode() & 0o022 != 0) {
            return Err(format!(
                "refusing to run untrusted privileged executable {}",
                path.display()
            ));
        }
        if privileged {
            for parent in path.ancestors().skip(1) {
                let metadata = parent.metadata().map_err(|error| {
                    format!("cannot validate directory {}: {error}", parent.display())
                })?;
                if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
                    return Err(format!(
                        "refusing privileged execution through untrusted directory {}",
                        parent.display()
                    ));
                }
            }
        }
        pinned[index] = path.display().to_string();
    }
    Ok(pinned)
}

pub fn make_noninteractive(mut command: Vec<String>, backend: &str) -> Vec<String> {
    let manager = if command
        .first()
        .and_then(|part| Path::new(part).file_name())
        .and_then(|part| part.to_str())
        .is_some_and(|part| ["sudo", "doas", "pkexec"].contains(&part))
    {
        command
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, part)| part.as_str() != "--")
            .map(|(index, _)| index)
            .unwrap_or(0)
    } else {
        0
    };
    match backend {
        "APT" | "APT-RPM" | "DNF" | "YUM" if !command.iter().any(|part| part == "-y") => {
            command.insert(manager + 1, "-y".to_owned())
        }
        "Zypper" if !command.iter().any(|part| part == "--non-interactive") => {
            command.insert(manager + 1, "--non-interactive".to_owned())
        }
        "Pacman" if !command.iter().any(|part| part == "--noconfirm") => {
            command.insert(manager + 1, "--noconfirm".to_owned())
        }
        _ => {}
    }
    command
}

pub fn rpm_backend() -> Option<&'static str> {
    rpm_manager()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_scope_and_data_are_explicit() {
        let mut item = Match::new("Flatpak", "org.example.App", "Example");
        item.scope = "user".to_owned();
        assert_eq!(
            command_for(&item, true).expect("command"),
            [
                "flatpak",
                "uninstall",
                "-y",
                "--user",
                "--delete-data",
                "org.example.App"
            ]
        );
    }

    #[test]
    fn named_flatpak_installation_is_preserved() {
        let mut item = Match::new("Flatpak", "org.example.App", "Example");
        item.installation = "work".to_owned();
        assert!(
            command_for(&item, false)
                .expect("command")
                .contains(&"--installation=work".to_owned())
        );
    }

    #[test]
    fn conda_removes_a_package_not_the_environment() {
        let mut item = Match::new("Conda", "ruff", "ruff");
        item.profile = "/home/me/env".to_owned();
        let command = command_for(&item, false).expect("command");
        assert_eq!(
            command,
            [
                "conda",
                "remove",
                "--yes",
                "--prefix",
                "/home/me/env",
                "ruff"
            ]
        );
        assert!(!command.contains(&"env".to_owned()));
    }

    #[test]
    fn guix_uses_the_discovered_profile() {
        let mut item = Match::new("Guix", "hello", "hello");
        item.profile = "/home/me/.guix-profile".to_owned();
        assert!(
            command_for(&item, false)
                .expect("command")
                .contains(&"--profile=/home/me/.guix-profile".to_owned())
        );
    }

    #[test]
    fn container_exports_are_blocked() {
        let item = Match::new("Container Export", "example.desktop", "Example");
        assert!(command_for(&item, false).is_err());
    }

    #[test]
    fn single_target_tool_managers_are_not_batched() {
        for backend in ["Pipx", "Cargo", "UV Tool"] {
            let first = Match::new(backend, "one", "one");
            let second = Match::new(backend, "two", "two");
            let batches = build_batches(&[first, second], &[]).expect("batches");
            assert_eq!(batches.len(), 2, "{backend}");
        }
    }

    #[test]
    fn exact_previews_enable_manager_noninteractive_mode() {
        let command = make_noninteractive(
            vec!["apt-get".to_owned(), "remove".to_owned(), "ed".to_owned()],
            "APT",
        );
        assert_eq!(command, ["apt-get", "-y", "remove", "ed"]);
    }
}
