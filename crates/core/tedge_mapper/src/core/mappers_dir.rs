//! Utilities for scanning the mappers directory and warning about misconfigured entries.
//!
//! A mapper directory under `/etc/tedge/mappers/` is valid if it is empty (the `flows/`
//! directory will be created automatically on startup), has a `flows/` subdirectory, has a
//! `mapper.toml`, or any combination thereof.
//!
//! The only configuration that is always broken is a `bridge/` directory without a
//! `mapper.toml`, since bridge rules require the connection settings from `mapper.toml`.
//! This generates a targeted warning pointing the user at the fix.

use camino::Utf8Path;
use tracing::warn;

/// Scans the mappers directory and emits warnings for directories that have a `bridge/`
/// subdirectory but no `mapper.toml`. These mappers will always fail to start because
/// bridge rules require the connection settings provided by `mapper.toml`.
///
/// Built-in mapper directories (`aws`, `az`, `c8y`, and profile variants like `aws.prod`)
/// are excluded because they source their configuration from `tedge.toml` and never use
/// `mapper.toml`.
///
/// Empty mapper directories and empty `flows/` directories are valid — the mapper creates
/// the `flows/` directory automatically on startup.
pub async fn warn_misconfigured_mapper_dirs(mappers_dir: &Utf8Path) {
    for name in collect_bridge_without_mapper_dirs(mappers_dir).await {
        warn!(
            "Mapper '{name}' has a 'bridge/' directory but no 'mapper.toml': the mapper will \
             fail to start. Add 'mapper.toml' to configure the connection or remove the \
             'bridge/' directory.",
        );
    }
}

/// Returns the names of mapper directories that have a `bridge/` subdirectory but no
/// `mapper.toml`. These mappers will always fail to start because bridge rules require
/// the connection settings provided by `mapper.toml`.
///
/// Built-in mapper directories (`aws`, `az`, `c8y`, and profile variants) are excluded
/// because they source their configuration from `tedge.toml`.
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
        if is_builtin_mapper_dir(&name) {
            continue;
        }
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

/// Returns `true` if the directory name corresponds to a built-in mapper (`aws`, `az`,
/// `c8y`) or a profiled variant thereof (e.g. `aws.prod`, `c8y.staging`).
fn is_builtin_mapper_dir(name: &str) -> bool {
    matches!(name, "c8y" | "az" | "aws")
        || matches!(name.split_once('.'), Some(("c8y" | "az" | "aws", _)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        async fn empty_mapper_dir_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard"))
                .await
                .unwrap();

            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                broken.is_empty(),
                "Empty mapper dir should not be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn empty_flows_dir_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();

            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                broken.is_empty(),
                "Empty flows/ dir should not be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn builtin_mapper_dir_with_bridge_but_no_toml_is_not_flagged() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            for name in ["aws", "az", "c8y", "aws.prod", "az.staging"] {
                tokio::fs::create_dir_all(mappers_dir.join(format!("{name}/bridge")))
                    .await
                    .unwrap();
            }
            let broken = collect_bridge_without_mapper_dirs(&mappers_dir).await;
            assert!(
                broken.is_empty(),
                "Built-in mapper dirs should not be flagged: {broken:?}"
            );
        }

        #[tokio::test]
        async fn handles_nonexistent_mappers_dir_gracefully() {
            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("nonexistent/mappers");
            assert!(collect_bridge_without_mapper_dirs(&mappers_dir)
                .await
                .is_empty());
        }

        #[tokio::test]
        async fn bridge_without_mapper_toml_emits_warn_log() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/bridge"))
                .await
                .unwrap();

            warn_misconfigured_mapper_dirs(&mappers_dir).await;

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

        #[tokio::test]
        async fn empty_dir_does_not_emit_warn_log() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("stale-dir"))
                .await
                .unwrap();

            warn_misconfigured_mapper_dirs(&mappers_dir).await;

            let captured = warnings.get();
            assert!(
                captured.is_empty(),
                "Empty mapper dir should not trigger any warnings, got: {captured:?}"
            );
        }

        #[tokio::test]
        async fn empty_flows_dir_does_not_emit_warn_log() {
            let warnings = CapturedWarnings::install();

            let ttd = TempTedgeDir::new();
            let mappers_dir = ttd.utf8_path().join("mappers");
            tokio::fs::create_dir_all(mappers_dir.join("thingsboard/flows"))
                .await
                .unwrap();

            warn_misconfigured_mapper_dirs(&mappers_dir).await;

            let captured = warnings.get();
            assert!(
                captured.is_empty(),
                "Empty flows/ dir should not trigger any warnings, got: {captured:?}"
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
