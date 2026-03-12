//! Utilities for scanning the mappers directory and warning about unrecognised entries.
//!
//! Under the no-prefix convention, a directory under `/etc/tedge/mappers/` is a mapper
//! if and only if it contains a `mapper.toml` file. Directories without `mapper.toml`
//! are unrecognised and generate a warning.

use camino::Utf8Path;
use tracing::warn;

/// Scans the mappers directory and emits a warning for each subdirectory
/// that does not contain a `mapper.toml` file.
///
/// This is called on mapper startup to help users spot typos or stale entries.
pub async fn warn_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) {
    for name in collect_unrecognised_mapper_dirs(mappers_dir).await {
        warn!(
            "Unrecognised mapper directory '{mappers_dir}/{name}': no 'mapper.toml' found. \
             This directory will be ignored.",
        );
    }
}

/// Returns the names of subdirectories in `mappers_dir` that do not contain a `mapper.toml`.
///
/// This is exposed for testing; callers should normally use `warn_unrecognised_mapper_dirs`.
pub(crate) async fn collect_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) -> Vec<String> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_dir).await else {
        return Vec::new(); // Directory doesn't exist yet — nothing to warn about
    };

    let mut unrecognised = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Built-in mappers (c8y, az, aws, collectd, local) and their profile
        // variants can legitimately have no mapper.toml — skip them silently.
        if crate::is_builtin_mapper_dir_name(&name) {
            continue;
        }
        let path = camino::Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        if !tokio::fs::try_exists(path.join("mapper.toml"))
            .await
            .unwrap_or(false)
        {
            unrecognised.push(name);
        }
    }
    unrecognised
}

#[cfg(test)]
mod tests {
    use super::*;

    mod warn_unrecognised {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn does_not_warn_for_dirs_with_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            for name in ["c8y", "az", "thingsboard", "production"] {
                let dir = mappers_dir.join(name);
                tokio::fs::create_dir_all(&dir).await.unwrap();
                tokio::fs::write(dir.join("mapper.toml"), "").await.unwrap();
            }

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.is_empty(),
                "Dirs with mapper.toml should not be unrecognised, got: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn handles_nonexistent_mappers_dir_gracefully() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("nonexistent/mappers");
            assert!(collect_unrecognised_mapper_dirs(&mappers_dir)
                .await
                .is_empty());
        }

        #[tokio::test]
        async fn dir_without_mapper_toml_is_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.contains(&"thingsboard".to_string()),
                "'thingsboard' (no mapper.toml) should be flagged: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn mixed_directories_only_flags_those_without_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");

            let c8y_dir = mappers_dir.join("c8y");
            tokio::fs::create_dir_all(&c8y_dir).await.unwrap();
            tokio::fs::write(c8y_dir.join("mapper.toml"), "")
                .await
                .unwrap();

            // No mapper.toml
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();
            tokio::fs::create_dir_all(mappers_dir.join("stale-dir"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert_eq!(
                unrecognised.len(),
                2,
                "Should flag exactly 2 unrecognised: {unrecognised:?}"
            );
            assert!(unrecognised.contains(&"thingsboard".to_string()));
            assert!(unrecognised.contains(&"stale-dir".to_string()));
        }

        #[tokio::test]
        async fn builtin_dir_without_mapper_toml_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // c8y dir without mapper.toml — simulates post-install before tedge config upgrade
            tokio::fs::create_dir_all(mappers_dir.join("c8y"))
                .await
                .unwrap();
            tokio::fs::create_dir_all(mappers_dir.join("az"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.is_empty(),
                "Built-in dirs without mapper.toml should not be flagged: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn profiled_builtin_dir_without_mapper_toml_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("c8y.prod"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.is_empty(),
                "Profiled built-in dir 'c8y.prod' without mapper.toml should not be flagged: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn name_with_builtin_prefix_but_not_a_profile_is_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // "c8y-extra" is not a profile (uses '-' not '.') — should be flagged
            tokio::fs::create_dir_all(mappers_dir.join("c8y-extra"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.contains(&"c8y-extra".to_string()),
                "'c8y-extra' (no dot separator, no mapper.toml) should be flagged: {unrecognised:?}"
            );
        }

        /// Verifies that `warn_unrecognised_mapper_dirs` actually emits `WARN` log events.
        #[tokio::test]
        async fn unrecognised_dirs_emit_warn_log_events() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("stale-dir"))
                .await
                .unwrap();
            // c8y without mapper.toml — built-in, should not warn
            tokio::fs::create_dir_all(mappers_dir.join("c8y"))
                .await
                .unwrap();

            warn_unrecognised_mapper_dirs(&mappers_dir).await;

            let captured = warnings.get();
            assert!(
                captured.iter().any(|m| m.contains("stale-dir")),
                "Expected a warning for 'stale-dir', got: {captured:?}"
            );
            assert!(
                captured.iter().all(|m| !m.contains("c8y")),
                "Should not warn about 'c8y' (built-in mapper), got: {captured:?}"
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
