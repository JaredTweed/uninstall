use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

pub fn norm(value: &str) -> String {
    value
        .nfkd()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if !character.is_control() && character != '\u{1b}' && character != '\u{7f}' {
                character
            } else {
                '?'
            }
        })
        .collect()
}

pub fn relevant(query: &str, values: &[&str]) -> bool {
    let needle = norm(query);
    if needle.is_empty() {
        return false;
    }
    values.iter().any(|value| {
        let normalized = norm(value);
        if needle.len() < 3 {
            needle == normalized
                || value
                    .rsplit('.')
                    .next()
                    .is_some_and(|tail| needle == norm(tail))
        } else {
            normalized.contains(&needle)
        }
    })
}

pub fn package_base(name: &str) -> String {
    let base = name.split(':').next().unwrap_or(name);
    const ARCHES: &[&str] = &[
        "aarch64", "alpha", "armv7hl", "armv7hnl", "i386", "i486", "i586", "i686", "ia64",
        "noarch", "ppc", "ppc64", "ppc64le", "riscv64", "s390", "s390x", "sparc", "src", "x86_64",
    ];
    if let Some((prefix, suffix)) = base.rsplit_once('.') {
        if ARCHES.contains(&suffix) {
            return prefix.to_owned();
        }
    }
    base.to_owned()
}

pub fn parse_size(value: &str) -> Option<u64> {
    static EXPRESSION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*(bytes?|[kmgtpe]?i?b)\b")
            .expect("valid size expression")
    });
    if let Some(found) = EXPRESSION.captures(&value.replace('\u{a0}', " ")) {
        let number: f64 = found.get(1)?.as_str().replace(',', ".").parse().ok()?;
        let power = match found.get(2)?.as_str().to_ascii_lowercase().as_str() {
            "b" | "byte" | "bytes" => 0,
            "kb" | "kib" => 1,
            "mb" | "mib" => 2,
            "gb" | "gib" => 3,
            "tb" | "tib" => 4,
            "pb" | "pib" => 5,
            "eb" | "eib" => 6,
            _ => return None,
        };
        return Some((number * 1024_f64.powi(power)) as u64);
    }
    value.trim().parse().ok()
}

pub fn format_size(bytes: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 10.0 {
        format!("{value:.0} {}", units[unit])
    } else {
        let rendered = format!("{value:.1}");
        format!(
            "{} {}",
            rendered.trim_end_matches('0').trim_end_matches('.'),
            units[unit]
        )
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub fn path_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "@%_+=:,./-".contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub fn command_string(command: &[String]) -> String {
    command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_case_and_diacritic_insensitive() {
        assert_eq!(norm("FrÉe-CAD"), "freecad");
    }

    #[test]
    fn terminal_controls_are_replaced() {
        assert_eq!(sanitize("safe\u{1b}[31m\n"), "safe?[31m?");
    }

    #[test]
    fn short_queries_require_an_exact_component() {
        assert!(!relevant("ed", &["editor"]));
        assert!(relevant("ed", &["org.example.ed"]));
    }

    #[test]
    fn longer_queries_allow_substring_matches() {
        assert!(relevant("freecad", &["org.freecad.FreeCAD"]));
    }

    #[test]
    fn rpm_architecture_is_removed_from_package_base() {
        assert_eq!(package_base("dosbox-staging.x86_64"), "dosbox-staging");
        assert_eq!(package_base("lib.example"), "lib.example");
    }

    #[test]
    fn size_parser_uses_binary_units() {
        assert_eq!(parse_size("1.5 GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("512 KiB"), Some(524_288));
    }

    #[test]
    fn size_formatter_is_concise() {
        assert_eq!(format_size(1_610_612_736), "1.5 GiB");
        assert_eq!(format_size(524_288), "512 KiB");
    }

    #[test]
    fn shell_display_quotes_metacharacters_without_executing_them() {
        assert_eq!(shell_quote("plain-name"), "plain-name");
        assert_eq!(shell_quote("$(touch bad)"), "'$(touch bad)'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn path_containment_is_component_aware() {
        assert!(path_within(
            Path::new("/home/user/app"),
            Path::new("/home/user")
        ));
        assert!(!path_within(
            Path::new("/home/username"),
            Path::new("/home/user")
        ));
    }
}
