use crate::command::Command;
use crate::error;
use crate::info;
use crate::log::MaybeFancy;
use crate::log::Spinner;
use crate::warning;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::process::ExitStatus;
use std::time::Duration;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_config::models::AbsolutePath;
use tedge_config::TEdgeConfig;
use tedge_utils::file;

#[derive(Debug)]
pub struct DiagCollectCommand {
    pub plugin_dir: AbsolutePath,
    pub config_dir: AbsolutePath,
    pub working_dir: AbsolutePath,
    pub diag_dir: AbsolutePath,
    pub tarball_name: String,
    pub keep_tmp_output: bool,
    pub graceful_timeout: Duration,
    pub forceful_timeout: Duration,
}

#[async_trait::async_trait]
impl Command for DiagCollectCommand {
    fn description(&self) -> String {
        "collect diagnostic information".into()
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let plugins = self.read_diag_plugins().await?;
        let plugin_count = plugins.len();
        if plugin_count == 0 {
            error!("No diagnostic plugins were found in {}", self.plugin_dir);
            std::process::exit(2)
        }

        file::create_directory_with_defaults(&self.diag_dir)
            .await
            .with_context(|| format!("failed to create directory at {}", self.diag_dir))?;

        let mut skipped_count = 0;
        let mut error_count = 0;

        for plugin in plugins {
            let banner = format!("Executing {plugin}");
            let spinner = Spinner::start(banner);
            let res = self.execute_diag_plugin(&plugin).await;

            match spinner.finish(res) {
                Ok(exit_status) if exit_status.success() => {}
                Ok(exit_status) if exit_status.code() == Some(2) => {
                    skipped_count += 1;
                    info!("{plugin} is marked skipped");
                }
                Ok(exit_status) => {
                    error_count += 1;
                    error!("{plugin} failed with exit status: {exit_status}");
                }
                Err(err) => {
                    error_count += 1;
                    error!("{plugin} failed with error: {err}");
                }
            }
        }

        let success_count = plugin_count - skipped_count - error_count;
        eprintln!("\nTotal {plugin_count} executed: {success_count} completed, {error_count} failed, {skipped_count} skipped");

        self.compress_into_a_tarball()
            .with_context(|| "Failed to compress diagnostic information")?;

        if !self.keep_tmp_output {
            tokio::fs::remove_dir_all(&self.diag_dir)
                .await
                .with_context(|| format!("Failed to delete directory: {}", self.diag_dir))?;
        }

        if error_count > 0 {
            std::process::exit(1)
        } else {
            Ok(())
        }
    }
}

impl DiagCollectCommand {
    async fn read_diag_plugins(&self) -> Result<BTreeSet<Utf8PathBuf>, anyhow::Error> {
        let mut plugins = BTreeSet::new();
        let mut entries = tokio::fs::read_dir(&self.plugin_dir)
            .await
            .with_context(|| format!("Failed to read directory: {}", self.plugin_dir))?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) {
                if path.is_file() && is_executable(&path).await {
                    plugins.insert(path);
                } else {
                    warning!("Skipping non-executable file: {:?}", entry.path());
                }
            } else {
                warning!("Ignoring invalid path: {:?}", entry.path());
            }
        }
        Ok(plugins)
    }

    async fn execute_diag_plugin(
        &self,
        plugin_path: &Utf8Path,
    ) -> Result<ExitStatus, anyhow::Error> {
        let plugin_name = plugin_path
            .file_stem()
            .with_context(|| format!("No file name for {}", plugin_path))?;
        let plugin_output_dir = self.diag_dir.join(plugin_name);
        let output_file = plugin_output_dir.join("output.log");
        file::create_directory_with_defaults(&plugin_output_dir)
            .await
            .with_context(|| format!("Failed to create output directory at {plugin_output_dir}"))?;

        let mut command = LoggedCommand::new(plugin_path, self.working_dir.to_path_buf())?;
        command
            .arg("collect")
            .arg("--output-dir")
            .arg(&plugin_output_dir)
            .arg("--config-dir")
            .arg(&self.config_dir);
        let child = command.spawn()?;
        let mut command_log =
            CommandLog::from_log_path(output_file, plugin_name.into(), "no cmd_id".into());
        let output = child
            .wait_for_output_with_timeout(
                &mut command_log,
                self.graceful_timeout,
                self.forceful_timeout,
            )
            .await?;
        Ok(output.status)
    }

    fn compress_into_a_tarball(&self) -> Result<Utf8PathBuf, anyhow::Error> {
        let mut tarball_path = self.diag_dir.to_path_buf();
        tarball_path.set_extension("tar.gz");

        // flate2 removed the support for async. Alternatively async-compression can be used?
        let tar_gz = std::fs::File::create(&tarball_path)?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = tar::Builder::new(enc);

        tar.append_dir_all(&self.tarball_name, &self.diag_dir)?;
        tar.finish()?;

        eprintln!("Diagnostic information saved to {tarball_path}");
        Ok(tarball_path)
    }
}

async fn is_executable(path: &Utf8Path) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::with_exec_permission;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn test_read_diag_plugins() {
        let ttd = TempTedgeDir::new();
        let command = DiagCollectCommand::new(&ttd);
        with_exec_permission(command.plugin_dir.join("plugin_a"), "pwd");
        with_exec_permission(command.plugin_dir.join("plugin_b"), "pwd");
        with_exec_permission(command.plugin_dir.join("plugin_c"), "pwd");

        let plugins = command.read_diag_plugins().await.unwrap();
        assert_eq!(plugins.len(), 3);
    }

    #[tokio::test]
    async fn read_diag_plugins_skips_non_executable_files_or_directories() {
        let ttd = TempTedgeDir::new();
        let command = DiagCollectCommand::new(&ttd);
        ttd.dir("plugins").file("plugin_a");
        ttd.dir("plugins").file("plugin_b");
        ttd.dir("plugins").dir("directory");
        with_exec_permission(command.plugin_dir.join("directory").join("plugin_c"), "pwd");

        let plugins = command.read_diag_plugins().await.unwrap();
        assert_eq!(plugins.len(), 0);
    }

    #[tokio::test]
    async fn test_execute_diag_plugins() {
        let ttd = TempTedgeDir::new();
        let command = DiagCollectCommand::new(&ttd);
        let plugin_a_path = command.plugin_dir.join("plugin_a.sh");
        with_exec_permission(command.plugin_dir.join(&plugin_a_path), "#!/bin/sh\nls");

        let status = command.execute_diag_plugin(&plugin_a_path).await.unwrap();
        assert!(status.success());

        let log_path = command.diag_dir.join("plugin_a").join("output.log");
        assert!(log_path.exists());

        let content = tokio::fs::read_to_string(log_path).await.unwrap();
        let expected_command = vec![
            plugin_a_path.to_string(),
            "collect".to_string(),
            "--output-dir".to_string(),
            command.diag_dir.join("plugin_a").to_string(),
            "--config-dir".to_string(),
            command.config_dir.to_string(),
        ];
        for item in expected_command {
            assert!(content.contains(item.as_str()));
        }
    }

    #[test]
    fn test_compress_tarball() {
        let ttd = TempTedgeDir::new();
        let command = DiagCollectCommand::new(&ttd);
        ttd.dir("tmp").dir("tarball").file("file1");
        ttd.dir("tmp").dir("tarball").file("file2");
        ttd.dir("tmp").dir("tarball").file("file3");
        let decompressed_dir = ttd.dir("decompressed");

        let tarball_path = command.compress_into_a_tarball().unwrap();
        assert!(tarball_path.exists());

        let tar_gz = std::fs::File::open(tarball_path).unwrap();
        let tar = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(decompressed_dir.path()).unwrap();

        assert!(decompressed_dir.path().join("tarball").is_dir());
        assert!(decompressed_dir
            .path()
            .join("tarball")
            .join("file1")
            .is_file());
        assert!(decompressed_dir
            .path()
            .join("tarball")
            .join("file2")
            .is_file());
        assert!(decompressed_dir
            .path()
            .join("tarball")
            .join("file3")
            .is_file());
    }

    impl DiagCollectCommand {
        fn new(ttd: &TempTedgeDir) -> Self {
            let plugin_dir = ttd.dir("plugins");
            let config_dir = ttd.dir("tedge");
            let working_dir = ttd.dir("tmp");
            let diag_dir = ttd.dir("tmp").dir("tarball");
            Self {
                plugin_dir: AbsolutePath::from_path(plugin_dir.utf8_path_buf()).unwrap(),
                config_dir: AbsolutePath::from_path(config_dir.utf8_path_buf()).unwrap(),
                working_dir: AbsolutePath::from_path(working_dir.utf8_path_buf()).unwrap(),
                diag_dir: AbsolutePath::from_path(diag_dir.utf8_path_buf()).unwrap(),
                tarball_name: "tarball".to_string(),
                keep_tmp_output: false,
                graceful_timeout: Duration::from_secs(60),
                forceful_timeout: Duration::from_secs(60),
            }
        }
    }
}
