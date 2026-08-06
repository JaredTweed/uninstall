use crate::command::exists;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeFamily {
    Apt,
    Rpm,
    Pacman,
    Apk,
    Xbps,
    Portage,
    Slackware,
    Eopkg,
    Swupd,
    Unknown,
}

fn os_release() -> &'static HashMap<String, String> {
    static RELEASE: OnceLock<HashMap<String, String>> = OnceLock::new();
    RELEASE.get_or_init(|| {
        let mut values = HashMap::new();
        let text = fs::read("/etc/os-release")
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            values.insert(key.to_owned(), value.trim_matches(['\'', '"']).to_owned());
        }
        values
    })
}

fn distro_ids() -> &'static HashSet<String> {
    static IDS: OnceLock<HashSet<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        let mut ids = HashSet::new();
        for key in ["ID", "ID_LIKE"] {
            if let Some(value) = os_release().get(key) {
                ids.extend(value.split_whitespace().map(str::to_owned));
            }
        }
        ids
    })
}

pub fn rpm_manager() -> Option<&'static str> {
    static MANAGER: OnceLock<Option<&'static str>> = OnceLock::new();
    *MANAGER.get_or_init(|| {
        let ids = distro_ids();
        if Path::new("/run/ostree-booted").exists() && exists("rpm-ostree") {
            return Some("RPM-OSTree");
        }
        if ids.contains("opensuse") || ids.contains("suse") || ids.contains("sles") {
            return exists("zypper").then_some("Zypper");
        }
        if ids.contains("mageia") || ids.contains("openmandriva") {
            return exists("urpme").then_some("URPMI");
        }
        if ids.contains("altlinux") && exists("apt-get") {
            return Some("APT-RPM");
        }
        if exists("dnf5") || exists("dnf") || exists("microdnf") {
            return Some("DNF");
        }
        if exists("yum") {
            return Some("YUM");
        }
        exists("rpm").then_some("RPM")
    })
}

pub fn dnf_binary() -> Option<&'static str> {
    static BINARY: OnceLock<Option<&'static str>> = OnceLock::new();
    *BINARY.get_or_init(|| {
        ["dnf5", "dnf", "microdnf"]
            .into_iter()
            .find(|name| exists(name))
    })
}

pub fn native_family() -> NativeFamily {
    static FAMILY: OnceLock<NativeFamily> = OnceLock::new();
    *FAMILY.get_or_init(|| {
        let ids = distro_ids();
        if ids
            .iter()
            .any(|id| ["debian", "ubuntu", "linuxmint", "pop"].contains(&id.as_str()))
        {
            NativeFamily::Apt
        } else if ids
            .iter()
            .any(|id| ["arch", "manjaro", "endeavouros"].contains(&id.as_str()))
        {
            NativeFamily::Pacman
        } else if ids.contains("alpine") {
            NativeFamily::Apk
        } else if ids.contains("void") {
            NativeFamily::Xbps
        } else if ids.contains("gentoo") {
            NativeFamily::Portage
        } else if ids.contains("slackware") {
            NativeFamily::Slackware
        } else if ids.contains("solus") {
            NativeFamily::Eopkg
        } else if ids.contains("clear-linux-os") {
            NativeFamily::Swupd
        } else if rpm_manager().is_some() {
            NativeFamily::Rpm
        } else if exists("dpkg-query") {
            NativeFamily::Apt
        } else if exists("pacman") {
            NativeFamily::Pacman
        } else if exists("apk") {
            NativeFamily::Apk
        } else if exists("xbps-query") {
            NativeFamily::Xbps
        } else {
            NativeFamily::Unknown
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_selection_is_stable_within_an_invocation() {
        assert_eq!(native_family(), native_family());
        assert_eq!(rpm_manager(), rpm_manager());
        assert_eq!(dnf_binary(), dnf_binary());
    }
}
