use serde::Serialize;
use std::path::PathBuf;

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
    pub backend: String,
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
    pub fn new(backend: &str, id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            backend: backend.to_owned(),
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
            self.backend, self.id, self.scope, self.installation, self.profile
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
    pub backend: String,
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
    }

    #[test]
    fn match_key_separates_profiles_and_installations() {
        let first = Match::new("Flatpak", "org.example.App", "Example");
        let mut second = first.clone();
        second.installation = "work".to_owned();
        assert_ne!(first.key(), second.key());
    }
}
