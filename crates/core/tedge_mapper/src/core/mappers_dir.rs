//! Utilities for scanning the mappers directory and warning about unrecognised entries.
//!
//! The `/etc/tedge/mappers/` directory may contain mapper directories for built-in mappers
//! (e.g. `c8y/`, `az.staging/`) and custom mapper directories (`custom/`, `custom.{name}/`).
//! Any directory that doesn't match these conventions may be a typo or a stale entry and
//! is worth warning about.

use camino::Utf8Path;
use tracing::warn;

use crate::MapperName;

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
fn classify_mapper_dir(name: &str) -> MapperDirKind {
    let (name, profile) = match name.split_once('.') {
        Some((name, profile)) => (name, Some(profile)),
        None => (name, None),
    };
    match name.parse::<MapperName>() {
        Ok(MapperName::Custom { .. }) if profile.is_none() => MapperDirKind::Custom,
        Ok(MapperName::Custom { .. }) => MapperDirKind::ProfiledCustom,
        Ok(_) if profile.is_none() => MapperDirKind::BuiltIn,
        Ok(_) => MapperDirKind::ProfiledBuiltIn,
        Err(_) => MapperDirKind::Unrecognised,
    }
}

/// Scans the mappers directory and emits a warning for each unrecognised subdirectory.
///
/// This is called on mapper startup to help users spot typos (e.g. `custome.thingsboard`)
pub fn warn_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) {
    for name in collect_unrecognised_mapper_dirs(mappers_dir) {
        warn!(
            "Unrecognised mapper directory '{mappers_dir}/{name}'. \
             Use 'custom.{{name}}/' for custom mappers; \
             run 'tedge-mapper --help' for built-in mapper types. \
             This directory will be ignored.",
        );
    }
}

/// Returns the names of unrecognised subdirectories found in `mappers_dir`.
///
/// A directory is unrecognised if its name is not a known mapper type or a profile of one.
/// This is exposed for testing; callers should normally use `warn_unrecognised_mapper_dirs`.
pub(crate) fn collect_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(mappers_dir) else {
        return Vec::new(); // Directory doesn't exist yet — nothing to warn about
    };

    let mut unrecognised = Vec::new();
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
            unrecognised.push(name.into_owned());
        }
    }
    unrecognised
}

#[cfg(test)]
mod tests {
    use super::*;

    mod classify {
        use super::*;

        #[test]
        fn builtin_names_are_recognised() {
            for name in ["c8y", "az", "aws", "local"] {
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
            // Typo: extra 'e' in the custom prefix
            assert_eq!(
                classify_mapper_dir("custome.thingsboard"),
                MapperDirKind::Unrecognised
            );
            // Unknown bare name (no built-in or custom prefix)
            assert_eq!(
                classify_mapper_dir("thingsboard"),
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

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir);
            assert!(
                unrecognised.is_empty(),
                "Known directories should not be unrecognised, got: {unrecognised:?}"
            );
        }

        #[test]
        fn handles_nonexistent_mappers_dir_gracefully() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("nonexistent/mappers");
            assert!(collect_unrecognised_mapper_dirs(&mappers_dir).is_empty());
        }

        #[test]
        fn typo_in_custom_prefix_triggers_warning() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // Typo: extra 'e' in custom
            std::fs::create_dir_all(mappers_dir.join("custome.thingsboard")).unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir);
            assert!(
                unrecognised.contains(&"custome.thingsboard".to_string()),
                "Typo 'custome.thingsboard' should be flagged as unrecognised: {unrecognised:?}"
            );
        }

        #[test]
        fn bare_unknown_name_triggers_warning() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            std::fs::create_dir_all(mappers_dir.join("thingsboard")).unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir);
            assert!(
                unrecognised.contains(&"thingsboard".to_string()),
                "Unknown bare name 'thingsboard' should be flagged as unrecognised: {unrecognised:?}"
            );
        }

        #[test]
        fn mixed_directories_only_flags_unrecognised_ones() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            std::fs::create_dir_all(mappers_dir.join("c8y")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("custom.thingsboard")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("custome.thingsboard")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("thingsboard")).unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir);
            assert_eq!(
                unrecognised.len(),
                2,
                "Should flag exactly 2 unrecognised: {unrecognised:?}"
            );
            assert!(unrecognised.contains(&"custome.thingsboard".to_string()));
            assert!(unrecognised.contains(&"thingsboard".to_string()));
        }

        /// Verifies that `warn_unrecognised_mapper_dirs` actually emits `WARN` log events.
        #[test]
        fn unrecognised_dirs_emit_warn_log_events() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            std::fs::create_dir_all(mappers_dir.join("custome.thingsboard")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("thingsboard")).unwrap();
            std::fs::create_dir_all(mappers_dir.join("c8y")).unwrap();

            warn_unrecognised_mapper_dirs(&mappers_dir);

            let captured = warnings.get();
            assert!(
                captured.iter().any(|m| m.contains("custome.thingsboard")),
                "Expected a warning for 'custome.thingsboard', got: {captured:?}"
            );
            assert!(
                captured
                    .iter()
                    .any(|m| m.contains("thingsboard") && !m.contains("custome")),
                "Expected a warning for 'thingsboard', got: {captured:?}"
            );
        }
    }

    /// Helper that installs a tracing subscriber capturing all WARN-level messages.
    /// The subscriber remains active until this struct is dropped.
    struct CapturedWarnings {
        messages: Arc<Mutex<Vec<String>>>,
        _guard: tracing::dispatcher::DefaultGuard,
    }

    use std::sync::Arc;
    use std::sync::Mutex;

    impl CapturedWarnings {
        fn install() -> Self {
            use tracing_subscriber::layer::SubscriberExt as _;
            use tracing_subscriber::util::SubscriberInitExt as _;
            let messages: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let layer = WarnCaptureLayer(messages.clone());
            let guard = tracing_subscriber::registry().with(layer).set_default();
            Self {
                messages,
                _guard: guard,
            }
        }

        fn get(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
            self.messages.lock().unwrap()
        }
    }

    struct WarnCaptureLayer(Arc<Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WarnCaptureLayer
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if *event.metadata().level() == tracing::Level::WARN {
                let mut visitor = MessageVisitor(String::new());
                event.record(&mut visitor);
                self.0.lock().unwrap().push(visitor.0);
            }
        }
    }

    struct MessageVisitor(String);

    impl tracing::field::Visit for MessageVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{value:?}");
            }
        }
    }
}
