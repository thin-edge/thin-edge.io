//! Utilities for scanning the mappers directory and warning about unrecognised entries.
//!
//! The `/etc/tedge/mappers/` directory may contain mapper directories for built-in mappers
//! (e.g. `c8y/`, `az.staging/`) and custom mapper directories (`custom/`, `custom.{name}/`).
//! Any directory that doesn't match these conventions may be a typo or a stale entry and
//! is worth warning about.

use camino::Utf8Path;
use tracing::warn;

/// Known built-in mapper type names.
const BUILTIN_MAPPERS: &[&str] = &["c8y", "az", "aws", "collectd", "local"];

/// Classifies a mapper directory name.
#[derive(Debug, PartialEq, Eq)]
pub enum MapperDirKind {
    /// A built-in mapper directory (e.g. `c8y/`).
    BuiltIn,
    /// A profiled built-in mapper directory (e.g. `c8y.staging/`).
    ProfiledBuiltIn,
    /// The default custom mapper directory (`custom/`).
    Custom,
    /// A profiled custom mapper directory (e.g. `custom.thingsboard/`).
    ProfiledCustom,
    /// An unrecognised directory that doesn't match any known convention.
    Unrecognised,
}

/// Classifies a single mapper directory name.
pub fn classify_mapper_dir(name: &str) -> MapperDirKind {
    if BUILTIN_MAPPERS.contains(&name) {
        return MapperDirKind::BuiltIn;
    }
    if name == "custom" {
        return MapperDirKind::Custom;
    }
    if let Some(profile_type) = name.split_once('.').map(|(t, _)| t) {
        if BUILTIN_MAPPERS.contains(&profile_type) {
            return MapperDirKind::ProfiledBuiltIn;
        }
        if profile_type == "custom" {
            return MapperDirKind::ProfiledCustom;
        }
    }
    MapperDirKind::Unrecognised
}

/// Scans the mappers directory and emits a warning for each unrecognised subdirectory.
///
/// This is called on mapper startup to help users spot typos (e.g. `custome.thingsboard`)
/// or stale directories from removed mappers.
pub fn warn_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) {
    let Ok(entries) = std::fs::read_dir(mappers_dir) else {
        return; // Directory doesn't exist yet — nothing to warn about
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(classify_mapper_dir(&name), MapperDirKind::Unrecognised) {
            warn!(
                "Unrecognised mapper directory '{}/{name}'. \
                 Expected built-in mappers (c8y, az, aws, collectd, local), \
                 their profiles (e.g. c8y.staging), \
                 or custom mappers (custom or custom.{{name}}). \
                 This directory will be ignored.",
                mappers_dir
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod classify {
        use super::*;

        #[test]
        fn builtin_names_are_recognised() {
            for name in BUILTIN_MAPPERS {
                assert_eq!(
                    classify_mapper_dir(name),
                    MapperDirKind::BuiltIn,
                    "{name} should be BuiltIn"
                );
            }
        }

        #[test]
        fn profiled_builtin_names_are_recognised() {
            assert_eq!(
                classify_mapper_dir("c8y.staging"),
                MapperDirKind::ProfiledBuiltIn
            );
            assert_eq!(
                classify_mapper_dir("az.prod"),
                MapperDirKind::ProfiledBuiltIn
            );
            assert_eq!(
                classify_mapper_dir("aws.us-east"),
                MapperDirKind::ProfiledBuiltIn
            );
        }

        #[test]
        fn custom_is_recognised() {
            assert_eq!(classify_mapper_dir("custom"), MapperDirKind::Custom);
        }

        #[test]
        fn profiled_custom_is_recognised() {
            assert_eq!(
                classify_mapper_dir("custom.thingsboard"),
                MapperDirKind::ProfiledCustom
            );
            assert_eq!(
                classify_mapper_dir("custom.my-cloud"),
                MapperDirKind::ProfiledCustom
            );
        }

        #[test]
        fn unrecognised_names_are_flagged() {
            // Typo: missing 's' in thingsboard
            assert_eq!(
                classify_mapper_dir("custome.thingsboard"),
                MapperDirKind::Unrecognised
            );
            // Unknown mapper type
            assert_eq!(
                classify_mapper_dir("mycloud"),
                MapperDirKind::Unrecognised
            );
            // Completely unknown
            assert_eq!(
                classify_mapper_dir("oldcloud"),
                MapperDirKind::Unrecognised
            );
        }
    }

    mod warn_unrecognised {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[test]
        fn does_not_warn_for_known_directories() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            std::fs::create_dir_all(mappers_dir.join("c8y")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("c8y.staging")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("custom.thingsboard")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("aws")).unwrap();

            // This should not panic or produce errors
            warn_unrecognised_mapper_dirs(&mappers_dir);
        }

        #[test]
        fn handles_nonexistent_mappers_dir_gracefully() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("nonexistent/mappers");
            // Should not panic
            warn_unrecognised_mapper_dirs(&mappers_dir);
        }
    }
}
