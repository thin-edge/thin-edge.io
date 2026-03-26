//! Utilities for scanning the mappers directory and warning about unrecognised entries.
//!
//! A directory under `/etc/tedge/mappers/` is a recognised mapper if it contains at least
//! one of: a `mapper.toml` file or a `flows/` subdirectory. Directories with neither are
//! unrecognised and generate a warning. Flows-only mapper directories (with `flows/` but no
//! `mapper.toml`) generate a separate warning if their `flows/` directory is empty.
//!
//! Mappers that have a `bridge/` directory but no `mapper.toml` will always fail to start,
//! since bridge rules require the connection settings from `mapper.toml`. These generate a
//! targeted warning pointing the user at the fix.

use camino::Utf8Path;
use tracing::warn;

/// Scans the mappers directory and emits warnings for:
/// - directories that have neither `mapper.toml` nor a `flows/` subdirectory (unrecognised)
/// - directories that have a `bridge/` subdirectory but no `mapper.toml` (will always fail
///   to start — bridge rules require the connection settings in `mapper.toml`)
/// - directories that have an empty `flows/` subdirectory and no `mapper.toml` (flows-only
///   mapper with no scripts defined)
///
/// This is called on mapper startup to help users spot typos or stale entries.
pub async fn warn_unrecognised_mapper_dirs(mappers_dir: &Utf8Path) {
    for name in collect_unrecognised_mapper_dirs(mappers_dir).await {
        warn!(
            "Unrecognised mapper directory '{mappers_dir}/{name}': no 'mapper.toml' or 'flows/' found. \
             This directory will be ignored.",
        );
    }
    for name in collect_bridge_without_mapper_dirs(mappers_dir).await {
        warn!(
            "Mapper '{name}' has a 'bridge/' directory but no 'mapper.toml': the mapper will \
             fail to start. Add 'mapper.toml' to configure the connection or remove the \
             'bridge/' directory.",
        );
    }
    for name in collect_empty_flows_mapper_dirs(mappers_dir).await {
        warn!("Mapper '{name}' has a 'flows/' directory but no flow scripts are defined.",);
    }
}

/// Returns the names of subdirectories in `mappers_dir` that have neither a `mapper.toml`
/// nor a `flows/` subdirectory.
///
/// A directory with a `flows/` subdirectory is a recognised flows-only mapper even without
/// `mapper.toml`. Only directories with neither are considered unrecognised.
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
        let path = camino::Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        let has_mapper_toml = tokio::fs::try_exists(path.join("mapper.toml"))
            .await
            .unwrap_or(false);
        let has_flows_dir = tokio::fs::try_exists(path.join("flows"))
            .await
            .unwrap_or(false);
        if !has_mapper_toml && !has_flows_dir {
            unrecognised.push(name);
        }
    }
    unrecognised
}

/// Returns the names of mapper directories that have a `bridge/` subdirectory but no
/// `mapper.toml`. These mappers will always fail to start because bridge rules require
/// the connection settings provided by `mapper.toml`.
///
/// This is exposed for testing; callers should normally use `warn_unrecognised_mapper_dirs`.
pub(crate) async fn collect_bridge_without_mapper_dirs(mappers_dir: &Utf8Path) -> Vec<String> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_dir).await else {
        return Vec::new();
    };

    let mut broken = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = camino::Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        let has_mapper_toml = tokio::fs::try_exists(path.join("mapper.toml"))
            .await
            .unwrap_or(false);
        let has_bridge_dir = tokio::fs::try_exists(path.join("bridge"))
            .await
            .unwrap_or(false);
        if has_bridge_dir && !has_mapper_toml {
            broken.push(name);
        }
    }
    broken
}

/// Returns the names of flows-only mapper directories whose `flows/` subdirectory is empty.
///
/// A directory qualifies when it has a `flows/` subdirectory but no `mapper.toml` and the
/// `flows/` directory contains no entries. This suggests the user set up the structure but
/// has not yet added any flow scripts.
///
/// This is exposed for testing; callers should normally use `warn_unrecognised_mapper_dirs`.
pub(crate) async fn collect_empty_flows_mapper_dirs(mappers_dir: &Utf8Path) -> Vec<String> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_dir).await else {
        return Vec::new();
    };

    let mut empty_flows = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = camino::Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        let has_mapper_toml = tokio::fs::try_exists(path.join("mapper.toml"))
            .await
            .unwrap_or(false);
        if has_mapper_toml {
            continue;
        }
        let Ok(mut flows_entries) = tokio::fs::read_dir(path.join("flows")).await else {
            continue; // no flows/ dir — handled by collect_unrecognised_mapper_dirs
        };
        if flows_entries.next_entry().await.ok().flatten().is_none() {
            empty_flows.push(name);
        }
    }
    empty_flows
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
        async fn dir_without_mapper_toml_or_flows_is_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.contains(&"thingsboard".to_string()),
                "'thingsboard' (no mapper.toml, no flows/) should be flagged: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn dir_with_flows_subdir_is_not_flagged_as_unrecognised() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // flows-only mapper: has flows/ but no mapper.toml
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();
            tokio::fs::write(
                mappers_dir.join("thingsboard/flows/telemetry.toml"),
                "input.mqtt.topics = [\"te/+/+/+/+/m/+\"]",
            )
            .await
            .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                !unrecognised.contains(&"thingsboard".to_string()),
                "'thingsboard' (flows-only) should not be flagged as unrecognised: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn dir_with_empty_flows_subdir_is_not_flagged_as_unrecognised() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // flows/ dir exists but is empty — still recognised (not unrecognised)
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                !unrecognised.contains(&"thingsboard".to_string()),
                "'thingsboard' (empty flows/) should not be flagged as unrecognised: {unrecognised:?}"
            );
        }

        #[tokio::test]
        async fn mixed_directories_only_flags_those_without_mapper_toml_or_flows() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");

            // c8y: simulates a real built-in mapper dir (has flows/, no mapper.toml)
            tokio::fs::create_dir_all(mappers_dir.join("c8y/flows"))
                .await
                .unwrap();

            // Neither mapper.toml nor flows/ — unrecognised
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
        async fn dir_without_mapper_toml_or_flows_is_flagged_regardless_of_name() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // Even a name that looks like a built-in is flagged if it has neither
            // mapper.toml nor flows/ — built-in mapper dirs always have flows/ in practice.
            tokio::fs::create_dir_all(mappers_dir.join("c8y-extra"))
                .await
                .unwrap();

            let unrecognised = collect_unrecognised_mapper_dirs(&mappers_dir).await;
            assert!(
                unrecognised.contains(&"c8y-extra".to_string()),
                "'c8y-extra' (no mapper.toml, no flows/) should be flagged: {unrecognised:?}"
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
            // c8y with flows/ and a flow script — simulates a real built-in mapper dir
            tokio::fs::create_dir_all(mappers_dir.join("c8y/flows"))
                .await
                .unwrap();
            tokio::fs::write(
                mappers_dir.join("c8y/flows/default.toml"),
                "input.mqtt.topics = [\"te/+/+/+/+/m/+\"]",
            )
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

        /// Verifies that `warn_unrecognised_mapper_dirs` emits the empty-flows warning.
        #[tokio::test]
        async fn empty_flows_dir_emits_warn_log_event() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            // flows-only mapper with empty flows/ — should warn about missing scripts
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();

            warn_unrecognised_mapper_dirs(&mappers_dir).await;

            let captured = warnings.get();
            assert!(
                captured.iter().any(|m| m.contains("thingsboard")),
                "Expected a warning for 'thingsboard' (empty flows/), got: {captured:?}"
            );
        }
    }

    mod bridge_without_mapper_toml {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn bridge_dir_without_mapper_toml_is_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/bridge"))
                .await
                .unwrap();

            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                broken.contains(&"thingsboard".to_string()),
                "'thingsboard' (bridge/ but no mapper.toml) should be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn bridge_dir_with_mapper_toml_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/bridge"))
                .await
                .unwrap();
            tokio::fs::write(mappers_dir.join("thingsboard/mapper.toml"), "")
                .await
                .unwrap();

            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                !broken.contains(&"thingsboard".to_string()),
                "'thingsboard' (bridge/ and mapper.toml present) should not be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn dir_without_bridge_subdir_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();

            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                broken.is_empty(),
                "Dir with no bridge/ should not be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn bridge_without_mapper_toml_emits_warn_log() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/bridge"))
                .await
                .unwrap();

            warn_unrecognised_mapper_dirs(&mappers_dir).await;

            let captured = warnings.get();
            assert!(
                captured.iter().any(|m| m.contains("thingsboard")),
                "Expected a warning mentioning 'thingsboard': {captured:?}"
            );
            assert!(
                captured.iter().any(|m| m.contains("bridge")),
                "Warning should mention 'bridge/': {captured:?}"
            );
        }
    }

    mod empty_flows {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn empty_flows_dir_without_mapper_toml_is_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();

            let empty_flows = collect_empty_flows_mapper_dirs(&mappers_dir).await;
            assert!(
                empty_flows.contains(&"thingsboard".to_string()),
                "'thingsboard' (empty flows/, no mapper.toml) should be flagged: {empty_flows:?}"
            );
        }

        #[tokio::test]
        async fn non_empty_flows_dir_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();
            tokio::fs::write(
                mappers_dir.join("thingsboard/flows/telemetry.toml"),
                "input.mqtt.topics = [\"te/+/+/+/+/m/+\"]",
            )
            .await
            .unwrap();

            let empty_flows = collect_empty_flows_mapper_dirs(&mappers_dir).await;
            assert!(
                !empty_flows.contains(&"thingsboard".to_string()),
                "'thingsboard' (non-empty flows/) should not be flagged: {empty_flows:?}"
            );
        }

        #[tokio::test]
        async fn mapper_toml_with_empty_flows_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();
            tokio::fs::write(mappers_dir.join("thingsboard/mapper.toml"), "")
                .await
                .unwrap();

            let empty_flows = collect_empty_flows_mapper_dirs(&mappers_dir).await;
            assert!(
                !empty_flows.contains(&"thingsboard".to_string()),
                "'thingsboard' (empty flows/ but has mapper.toml) should not be flagged: {empty_flows:?}"
            );
        }

        #[tokio::test]
        async fn dir_without_flows_subdir_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();

            let empty_flows = collect_empty_flows_mapper_dirs(&mappers_dir).await;
            assert!(
                empty_flows.is_empty(),
                "Dir with no flows/ at all should not appear in empty-flows list: {empty_flows:?}"
            );
        }

        #[tokio::test]
        async fn handles_nonexistent_mappers_dir_gracefully() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("nonexistent/mappers");
            assert!(collect_empty_flows_mapper_dirs(&mappers_dir)
                .await
                .is_empty());
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
