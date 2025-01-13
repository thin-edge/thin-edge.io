use camino::Utf8Path;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use crate::cli::LogConfigArgs;
use crate::system_services::SystemConfig;
use crate::system_services::SystemServiceError;
use std::io::IsTerminal;
use std::str::FromStr;

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
) -> Result<(), SystemServiceError> {
    let print_file_and_line = std::env::var("RUST_LOG").is_ok();
    let file_level = get_log_level(sname, config_dir)?;

    let filter_layer = filter_layer(flags, file_level);

    // print code location if RUST_LOG is used
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_span_events(FmtSpan::NONE)
        .with_file(print_file_and_line)
        .with_line_number(print_file_and_line)
        .with_filter(filter_layer);

    tracing_subscriber::registry().with(fmt_layer).init();

    Ok(())
}

fn filter_layer(flags: &LogConfigArgs, file_level: tracing::Level) -> EnvFilter {
    // 1. use level from flags if they're present
    let log_level = flags
        .log_level
        .or(flags.debug.then_some(tracing::Level::DEBUG));
    if let Some(log_level) = log_level {
        return EnvFilter::new(log_level.to_string());
    }

    // 2. if not, use RUST_LOG
    if std::env::var("RUST_LOG").is_ok() {
        return EnvFilter::from_default_env();
    }

    // 3. if not, use file content (info if no logging preferences in file)
    EnvFilter::new(file_level.to_string())
}

pub fn get_log_level(
    sname: &str,
    config_dir: &Utf8Path,
) -> Result<tracing::Level, SystemServiceError> {
    let loglevel = SystemConfig::try_new(config_dir)?.log;
    match loglevel.get(sname) {
        Some(ll) => tracing::Level::from_str(&ll.to_uppercase()).map_err(|_| {
            SystemServiceError::InvalidLogLevel {
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
    use std::io::Write;
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
        let mut file = std::fs::File::create(config_file_path.as_path())?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, config_root.try_into().unwrap()))
    }
}
