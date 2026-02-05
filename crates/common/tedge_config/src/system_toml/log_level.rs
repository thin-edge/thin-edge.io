use super::SystemConfig;
use super::SystemTomlError;
use crate::cli::LogConfigArgs;
use camino::Utf8Path;
use std::io::IsTerminal;
use std::str::FromStr;
use std::sync::Arc;
use tracing_subscriber::util::SubscriberInitExt;

#[macro_export]
/// The basic subscriber
macro_rules! subscriber_builder {
    () => {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
    };
}

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
    let logger = logger(sname, flags, Some(config_dir))?;
    logger.init();
    Ok(())
}

pub fn unconfigured_logger() -> Arc<dyn tracing::Subscriber + Send + Sync> {
    logger(
        "tedge",
        &LogConfigArgs {
            debug: false,
            log_level: None,
        },
        None,
    )
    .unwrap()
}

fn logger(
    sname: &str,
    flags: &LogConfigArgs,
    config_dir: Option<&Utf8Path>,
) -> Result<Arc<dyn tracing::Subscriber + Send + Sync>, SystemTomlError> {
    let subscriber = subscriber_builder!();

    let log_level = flags
        .log_level
        .or(flags.debug.then_some(tracing::Level::DEBUG));

    if let Some(log_level) = log_level {
        return Ok(Arc::new(subscriber.with_max_level(log_level).finish()));
    }

    if std::env::var("RUST_LOG").is_ok() {
        return Ok(Arc::new(
            subscriber
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_file(true)
                .with_line_number(true)
                .finish(),
        ));
    }

    if let Some(log_level) = config_dir.and_then(|config_dir| {
        get_log_level_from_config_file(sname, config_dir)
            .ok()
            .flatten()
    }) {
        return Ok(Arc::new(subscriber.with_max_level(log_level).finish()));
    }

    Ok(Arc::new(
        subscriber.with_max_level(DEFAULT_MAX_LEVEL).finish(),
    ))
}

const DEFAULT_MAX_LEVEL: tracing::Level = tracing::Level::INFO;

/// Return the log level for a given service, if it's defined in the config file. Otherwise return `None`.
pub fn get_log_level_from_config_file(
    sname: &str,
    config_dir: &Utf8Path,
) -> Result<Option<tracing::Level>, SystemTomlError> {
    let loglevel = SystemConfig::try_new(config_dir)?.log;
    match loglevel.get(sname) {
        Some(ll) => {
            let ll = tracing::Level::from_str(&ll.to_uppercase()).map_err(|_| {
                SystemTomlError::InvalidLogLevel {
                    name: ll.to_string(),
                }
            })?;
            Ok(Some(ll))
        }
        None => Ok(None),
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
        .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
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
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir)?;
        assert_eq!(Some(Level::DEBUG), res);
        Ok(())
    }

    #[test]
    fn invalid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "other"
    "#;
        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap_err();
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
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap_err();

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
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap();
        assert_eq!(None, res);
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
