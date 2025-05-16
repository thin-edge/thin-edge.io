use crate::command::Command;
use crate::log::MaybeFancy;
use crate::log::Spinner;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::process::ExitStatus;
use std::time::Duration;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_utils::file;
use yansi::Paint;

pub struct DiagCollectCommand {
    pub plugin_dir: Utf8PathBuf,
    pub diag_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub graceful_timeout: Duration,
    pub forceful_timeout: Duration,
}

#[async_trait::async_trait]
impl Command for DiagCollectCommand {
    fn description(&self) -> String {
        "collect diagnostic information".into()
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        let plugins = self.init().await?;
        let plugin_count = plugins.len();
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
                    println!("{}", format!("INFO: {plugin} is marked skipped").yellow());
                }
                Ok(exit_status) => {
                    error_count += 1;
                    println!(
                        "{}",
                        format!("ERROR: {plugin} failed with exit status: {exit_status}").red()
                    );
                }
                Err(err) => {
                    error_count += 1;
                    println!(
                        "{}",
                        format!("ERROR: {plugin} failed with error: {err}").red()
                    );
                }
            }
        }

        let success_count = plugin_count - skipped_count - error_count;
        println!("Total {plugin_count} executed: {success_count} completed, {error_count} failed, {skipped_count} skipped");

        if success_count > 0 {
            self.compress_into_a_tarball()
                .with_context(|| "Failed to compress diagnostic information")?;
        }

        if error_count > 0 {
            std::process::exit(1)
        } else if skipped_count > 0 {
            std::process::exit(2)
        } else {
            Ok(())
        }
    }
}

impl DiagCollectCommand {
    async fn execute_diag_plugin(
        &self,
        plugin_path: &Utf8Path,
    ) -> Result<ExitStatus, anyhow::Error> {
        let plugin_name = plugin_path.file_stem().context("No filename")?;
        let plugin_output_dir = self.diag_dir.join(plugin_name);
        let plugin_absolute_path = plugin_path.canonicalize()?;
        let output_file = plugin_output_dir.join("output.log");
        file::create_directory_with_defaults(&plugin_output_dir)
            .await
            .with_context(|| format!("Failed to create output directory at {plugin_output_dir}"))?;

        let mut command = LoggedCommand::new(&plugin_absolute_path)?;
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
        let mut tarball = self.diag_dir.clone();
        tarball.set_extension("tar.gz");
        let tar_gz = std::fs::File::create(&tarball)
            .with_context(|| format!("Failed to create {tarball}"))?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.append_dir_all("", &self.diag_dir)?;
        tar.finish()?;
        eprintln!("Diagnostic information saved to {tarball}");
        Ok(tarball)
    }

    async fn init(&self) -> Result<HashSet<Utf8PathBuf>, anyhow::Error> {
        file::create_directory_with_defaults(&self.diag_dir).await?;
        let plugins = self.scan_diag_plugins().await.with_context(|| {
            format!(
                "Failed to scan diag plugin directory {:?}",
                &self.plugin_dir
            )
        })?;
        Ok(plugins)
    }

    async fn scan_diag_plugins(&self) -> Result<HashSet<Utf8PathBuf>, anyhow::Error> {
        let mut plugins = HashSet::new();
        let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
            if path.is_file() && is_executable(&path).await {
                plugins.insert(path);
            }
        }
        Ok(plugins)
    }
}

async fn is_executable(path: &Utf8Path) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}
