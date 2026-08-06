use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Backend {
    #[serde(rename = "Flatpak")]
    Flatpak,
    #[serde(rename = "Snap")]
    Snap,
    #[serde(rename = "APT")]
    Apt,
    #[serde(rename = "APT-RPM")]
    AptRpm,
    #[serde(rename = "DNF")]
    Dnf,
    #[serde(rename = "YUM")]
    Yum,
    #[serde(rename = "RPM")]
    Rpm,
    #[serde(rename = "RPM-OSTree")]
    RpmOstree,
    #[serde(rename = "Zypper")]
    Zypper,
    #[serde(rename = "URPMI")]
    Urpmi,
    #[serde(rename = "Pacman")]
    Pacman,
    #[serde(rename = "APK")]
    Apk,
    #[serde(rename = "OPKG")]
    Opkg,
    #[serde(rename = "XBPS")]
    Xbps,
    #[serde(rename = "Portage")]
    Portage,
    #[serde(rename = "Slackware")]
    Slackware,
    #[serde(rename = "Eopkg")]
    Eopkg,
    #[serde(rename = "Swupd")]
    Swupd,
    #[serde(rename = "Swupd 3rd-party")]
    SwupdThirdParty,
    #[serde(rename = "Homebrew")]
    Homebrew,
    #[serde(rename = "Homebrew Cask")]
    HomebrewCask,
    #[serde(rename = "Gear Lever")]
    GearLever,
    #[serde(rename = "Pipx")]
    Pipx,
    #[serde(rename = "UV Tool")]
    UvTool,
    #[serde(rename = "NPM")]
    Npm,
    #[serde(rename = "Cargo")]
    Cargo,
    #[serde(rename = "Nix")]
    Nix,
    #[serde(rename = "Nix Legacy")]
    NixLegacy,
    #[serde(rename = "Guix")]
    Guix,
    #[serde(rename = "Conda")]
    Conda,
    #[serde(rename = "Micromamba")]
    Micromamba,
    #[serde(rename = "AppImage")]
    AppImage,
    #[serde(rename = "Standalone")]
    Standalone,
    #[serde(rename = "Container Export")]
    ContainerExport,
}

impl Backend {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flatpak => "Flatpak",
            Self::Snap => "Snap",
            Self::Apt => "APT",
            Self::AptRpm => "APT-RPM",
            Self::Dnf => "DNF",
            Self::Yum => "YUM",
            Self::Rpm => "RPM",
            Self::RpmOstree => "RPM-OSTree",
            Self::Zypper => "Zypper",
            Self::Urpmi => "URPMI",
            Self::Pacman => "Pacman",
            Self::Apk => "APK",
            Self::Opkg => "OPKG",
            Self::Xbps => "XBPS",
            Self::Portage => "Portage",
            Self::Slackware => "Slackware",
            Self::Eopkg => "Eopkg",
            Self::Swupd => "Swupd",
            Self::SwupdThirdParty => "Swupd 3rd-party",
            Self::Homebrew => "Homebrew",
            Self::HomebrewCask => "Homebrew Cask",
            Self::GearLever => "Gear Lever",
            Self::Pipx => "Pipx",
            Self::UvTool => "UV Tool",
            Self::Npm => "NPM",
            Self::Cargo => "Cargo",
            Self::Nix => "Nix",
            Self::NixLegacy => "Nix Legacy",
            Self::Guix => "Guix",
            Self::Conda => "Conda",
            Self::Micromamba => "Micromamba",
            Self::AppImage => "AppImage",
            Self::Standalone => "Standalone",
            Self::ContainerExport => "Container Export",
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.label()
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|backend| backend.label().eq_ignore_ascii_case(value))
    }

    pub const ALL: [Self; 34] = [
        Self::Flatpak,
        Self::Snap,
        Self::Apt,
        Self::AptRpm,
        Self::Dnf,
        Self::Yum,
        Self::Rpm,
        Self::RpmOstree,
        Self::Zypper,
        Self::Urpmi,
        Self::Pacman,
        Self::Apk,
        Self::Opkg,
        Self::Xbps,
        Self::Portage,
        Self::Slackware,
        Self::Eopkg,
        Self::Swupd,
        Self::SwupdThirdParty,
        Self::Homebrew,
        Self::HomebrewCask,
        Self::GearLever,
        Self::Pipx,
        Self::UvTool,
        Self::Npm,
        Self::Cargo,
        Self::Nix,
        Self::NixLegacy,
        Self::Guix,
        Self::Conda,
        Self::Micromamba,
        Self::AppImage,
        Self::Standalone,
        Self::ContainerExport,
    ];
}

impl std::fmt::Display for Backend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Explicit,
    External,
    Group,
    Dependency,
    WeakDependency,
    Unknown,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::External => "external",
            Self::Group => "group",
            Self::Dependency => "dependency",
            Self::WeakDependency => "weak dependency",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_dependency(self) -> bool {
        matches!(self, Self::Dependency | Self::WeakDependency)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Match {
    pub backend: Backend,
    pub id: String,
    pub name: String,
    pub version: String,
    pub scope: String,
    pub role: Role,
    pub reason: String,
    pub command_path: Option<PathBuf>,
    pub source_path: Option<PathBuf>,
    pub installed_size_bytes: Option<u64>,
    pub architecture: String,
    pub profile: String,
    pub installation: String,
    pub origin: String,
    pub summary: String,
    pub evidence: String,
}

impl Match {
    pub fn new(backend: Backend, id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            backend,
            id: id.into(),
            name: name.into(),
            version: String::new(),
            scope: "system".to_owned(),
            role: Role::Unknown,
            reason: String::new(),
            command_path: None,
            source_path: None,
            installed_size_bytes: None,
            architecture: String::new(),
            profile: String::new(),
            installation: String::new(),
            origin: String::new(),
            summary: String::new(),
            evidence: String::new(),
        }
    }

    pub fn key(&self) -> String {
        format!(
            "{}\0{}\0{}\0{}\0{}",
            self.backend.label(),
            self.id,
            self.scope,
            self.installation,
            self.profile
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreviewStatus {
    Exact,
    Blocked,
    NoOp,
    Unknown,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Impact {
    Low,
    Caution,
    High,
    Unknown,
    Blocked,
}

impl Impact {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Caution => "CAUTION",
            Self::High => "HIGH",
            Self::Unknown => "UNKNOWN",
            Self::Blocked => "BLOCKED",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub status: PreviewStatus,
    pub impact: Impact,
    pub planned_removals: Vec<String>,
    pub unused_dependencies: Vec<String>,
    pub protected: Vec<String>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
    pub fingerprint: String,
}

impl Preview {
    pub fn exact(requested: Vec<String>) -> Self {
        Self {
            status: PreviewStatus::Exact,
            impact: Impact::Low,
            planned_removals: requested,
            unused_dependencies: Vec::new(),
            protected: Vec::new(),
            blockers: Vec::new(),
            notes: Vec::new(),
            fingerprint: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemovalBatch {
    pub items: Vec<Match>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonResult {
    pub backend: Backend,
    pub id: String,
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub scope: String,
    pub profile: String,
    pub installation: String,
    pub command_path: String,
    pub role: Role,
    pub why_installed: String,
    pub installed_size_bytes: Option<u64>,
    pub preview: Preview,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dependency_roles_are_hidden_candidates() {
        assert!(Role::Dependency.is_dependency());
        assert!(Role::WeakDependency.is_dependency());
        assert!(!Role::Group.is_dependency());
        assert!(!Role::Explicit.is_dependency());
    }

    #[test]
    fn labels_are_human_readable() {
        assert_eq!(Role::WeakDependency.label(), "weak dependency");
        assert_eq!(Impact::Caution.label(), "CAUTION");
        assert_eq!(Backend::RpmOstree.label(), "RPM-OSTree");
        assert_eq!(Backend::parse("gear lever"), Some(Backend::GearLever));
        assert_eq!(
            serde_json::to_string(&Backend::SwupdThirdParty).expect("serialize backend"),
            "\"Swupd 3rd-party\""
        );
    }

    #[test]
    fn match_key_separates_profiles_and_installations() {
        let first = Match::new(Backend::Flatpak, "org.example.App", "Example");
        let mut second = first.clone();
        second.installation = "work".to_owned();
        assert_ne!(first.key(), second.key());
    }
}
