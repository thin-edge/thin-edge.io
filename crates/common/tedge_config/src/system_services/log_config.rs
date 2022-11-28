use crate::system_services::{SystemConfig, SystemServiceError};
use std::path::PathBuf;
use std::str::FromStr;

pub fn get_log_level(
    sname: &str,
    config_dir: PathBuf,
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

/// Initialize a `tracing_subscriber`
/// Reports all the log events sent either with the `log` crate or the `tracing` crate.
pub fn set_log_level(log_level: tracing::Level) {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_max_level(log_level)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let res = get_log_level("tedge_mapper", config_dir)?;
        assert_eq!(Level::DEBUG, res);
        Ok(())
    }

    #[test]
    fn invalid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "infoo"
    "#;
        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", config_dir).unwrap_err();
        assert_eq!(
            "Invalid log level: \"infoo\", supported levels are info, warn, error and debug",
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
        let res = get_log_level("tedge_mapper", config_dir).unwrap_err();

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
        some_mapper = "infoo"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level("tedge_mapper", config_dir).unwrap();
        assert_eq!(Level::INFO, res);
        Ok(())
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_system_config(content: &str) -> std::io::Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = temp_dir.path().to_path_buf();
        let config_file_path = config_root.join("system.toml");
        let mut file = std::fs::File::create(config_file_path.as_path())?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, config_root))
    }
}
