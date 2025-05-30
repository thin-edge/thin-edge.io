use camino::Utf8Path;

use super::SystemConfig;
use super::SystemTomlError;
use crate::cli::LogConfigArgs;
use std::io::IsTerminal;
use std::str::FromStr;
use tracing::metadata::LevelFilter;
use tracing_appender::rolling::*;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

/// Configures and enables logging taking into account flags, env variables and file config.
///
/// 1. Log config is taken from the file configuration first
/// 2. If `RUST_LOG` variable is set, it overrides file-based configuration
/// 3. If `--debug` or `--log-level` flags are set, they override previous steps
///
/// Reports all the log events sent either with the `log` crate or the `tracing`
/// crate.
pub fn log_init(
    sname: &str,
    flags: &LogConfigArgs,
    config_dir: &Utf8Path,
) -> Result<(), SystemTomlError> {
    // General logging
    let log_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339());

    let log_level = flags
        .log_level
        .or(flags.debug.then_some(tracing::Level::DEBUG));

    let log_layer = if let Some(log_level) = log_level {
        log_layer
            .with_filter(LevelFilter::from_level(log_level))
            .boxed()
    } else if std::env::var("RUST_LOG").is_ok() {
        log_layer
            .with_file(true)
            .with_line_number(true)
            .with_filter(tracing_subscriber::EnvFilter::from_default_env())
            .boxed()
    } else {
        let log_level = get_log_level(sname, config_dir)?;
        log_layer
            .with_filter(LevelFilter::from_level(log_level))
            .boxed()
    };

    // Audit journal
    let audit_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("tedge.audit.log")
        .max_log_files(7);
    let audit_layer = audit_appender
        .build("/var/log/tedge")
        .ok()
        .map(|audit_appender| {
            tracing_subscriber::fmt::layer()
                .with_writer(audit_appender)
                .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
                .with_filter(LevelFilter::INFO)
                .with_filter(filter_fn(|metadata| metadata.target() == "Audit"))
        });

    // Actor traces
    let trace_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("tedge.actors.log")
        .max_log_files(2);
    let trace_layer = trace_appender
        .build("/var/log/tedge")
        .ok()
        .map(|trace_appender| {
            tracing_subscriber::fmt::layer()
                .with_writer(trace_appender)
                .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
                .with_filter(LevelFilter::DEBUG)
                .with_filter(filter_fn(|metadata| metadata.target() == "Actors"))
        });

    tracing_subscriber::registry()
        .with(audit_layer)
        .with(trace_layer)
        .with(log_layer)
        .init();

    Ok(())
}

pub fn get_log_level(
    sname: &str,
    config_dir: &Utf8Path,
) -> Result<tracing::Level, SystemTomlError> {
    let loglevel = SystemConfig::try_new(config_dir)?.log;
    match loglevel.get(sname) {
        Some(ll) => tracing::Level::from_str(&ll.to_uppercase()).map_err(|_| {
            SystemTomlError::InvalidLogLevel {
                name: ll.to_string(),
            }
        }),
        None => Ok(tracing::Level::INFO),
    }
}

/// Initializes a tracing subscriber with a given log level if environment
/// variable `RUST_LOG` is not present.
///
/// Reports all the log events sent either with the `log` crate or the `tracing`
/// crate.
pub fn set_log_level(log_level: tracing::Level) {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339());

    if std::env::var("RUST_LOG").is_ok() {
        subscriber
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    } else {
        subscriber.with_max_level(log_level).init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;
    use tracing::Level;

    #[test]
    fn valid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", &config_dir)?;
        assert_eq!(Level::DEBUG, res);
        Ok(())
    }

    #[test]
    fn invalid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "other"
    "#;
        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", &config_dir).unwrap_err();
        assert_eq!(
            "Invalid log level: \"other\", supported levels are info, warn, error and debug",
            res.to_string()
        );
        Ok(())
    }

    #[test]
    fn empty_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = ""
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", &config_dir).unwrap_err();

        assert_eq!(
            "Invalid log level: \"\", supported levels are info, warn, error and debug",
            res.to_string()
        );
        Ok(())
    }

    #[test]
    fn log_level_not_configured_for_the_service() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        some_mapper = "other"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", &config_dir).unwrap();
        assert_eq!(Level::INFO, res);
        Ok(())
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_system_config(content: &str) -> std::io::Result<(TempDir, Utf8PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = temp_dir.path().to_path_buf();
        let config_file_path = config_root.join("system.toml");
        std::fs::write(config_file_path.as_path(), content.as_bytes())?;
        Ok((temp_dir, config_root.try_into().unwrap()))
    }
}
