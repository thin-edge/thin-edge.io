use super::log::MaybeFancy;
use crate::command::Command;
use crate::Component;
use anyhow::bail;
use anyhow::Context;
use clap::Subcommand;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::TEdgeConfig;
use tedge_utils::file::change_user_and_group;
use tedge_utils::file::create_directory;
use tedge_utils::file::PermissionEntry;
use tracing::debug;

pub struct TEdgeInitCmd {
    user: String,
    group: String,
    relative_links: bool,
}

impl TEdgeInitCmd {
    pub fn new(user: String, group: String, relative_links: bool) -> Self {
        Self {
            user,
            group,
            relative_links,
        }
    }
}

impl TEdgeInitCmd {
    async fn initialize_tedge(&self, config: TEdgeConfig) -> anyhow::Result<()> {
        let executable_name =
            std::env::current_exe().context("retrieving the current executable name")?;
        let stat = tokio::fs::metadata(&executable_name)
            .await
            .with_context(|| {
                format!(
                    "reading metadata for the current executable ({})",
                    executable_name.display()
                )
            })?;
        let Some(executable_dir) = executable_name.parent() else {
            bail!(
                "current executable ({}) does not have a parent directory",
                executable_name.display()
            )
        };
        let Some(executable_file_name) = executable_name.file_name() else {
            bail!(
                "current executable ({}) does not have a file name",
                executable_name.display()
            )
        };

        let component_subcommands: Vec<String> =
            Component::augment_subcommands(clap::Command::new("tedge"))
                .get_subcommands()
                .map(|c| c.get_name().to_owned())
                .chain(["tedge-apt-plugin".to_owned()])
                .collect();

        let target = Target {
            path: match self.relative_links {
                true => Path::new(executable_file_name),
                false => &executable_name,
            },
            uid: stat.uid(),
            gid: stat.gid(),
        };

        for component in &component_subcommands {
            create_symlinks_for(component, target, executable_dir, &RealEnv).await?;
        }

        let config_dir = &config.root_dir();
        let permissions = {
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            )
        };
        create_directory(&config_dir, &permissions).await?;
        create_directory(config_dir.join("mosquitto-conf"), &permissions).await?;
        create_directory(config_dir.join("operations"), &permissions).await?;
        create_directory(config_dir.join("operations").join("c8y"), &permissions).await?;
        create_directory(config_dir.join("plugins"), &permissions).await?;
        create_directory(config_dir.join("sm-plugins"), &permissions).await?;
        create_directory(config_dir.join("device-certs"), &permissions).await?;
        create_directory(config_dir.join(".tedge-mapper-c8y"), &permissions).await?;

        create_directory(&config.logs.path, &permissions).await?;
        create_directory(&config.data.path, &permissions).await?;
        create_directory(&config.share.path.join("tedge"), &permissions).await?;

        for log_plugins_dir in &config.log.plugin_paths.0 {
            create_directory(&log_plugins_dir, &permissions).await?;
        }

        let entity_store_file = config_dir.join(".agent").join("entity_store.jsonl");

        if entity_store_file.exists() {
            change_user_and_group(
                entity_store_file.into(),
                self.user.to_string(),
                self.group.to_string(),
            )
            .await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Command for TEdgeInitCmd {
    fn description(&self) -> String {
        "Initialize tedge".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.initialize_tedge(config)
            .await
            .with_context(|| "Failed to initialize tedge. You have to run tedge with sudo.")
            .map_err(<_>::into)
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
trait FileSystem {
    async fn read_link(&self, link: &Path) -> std::io::Result<PathBuf> {
        match tokio::fs::read_link(link).await {
            // File exists, but it's not a symlink
            Err(err) if err.kind() == ErrorKind::InvalidInput => Ok(link.to_owned()),
            res => res,
        }
    }

    async fn unlink(&self, link: &Path) -> nix::Result<()> {
        let link = link.to_path_buf();
        tokio::task::spawn_blocking(move || nix::unistd::unlink(&link))
            .await
            .expect("unlinking failed")
    }

    async fn symlink(&self, original: &Path, link: &Path) -> std::io::Result<()> {
        tokio::fs::symlink(original, link).await
    }

    async fn chown_symlink(&self, link: &Path, uid: u32, gid: u32) -> anyhow::Result<()> {
        // Use -h over --no-dereference as the former is supported in more environments,
        // busybox, bsd etc.
        let res = tokio::process::Command::new("chown")
            .arg("-h")
            .arg(format!("{uid}:{gid}"))
            .arg(link)
            .output()
            .await
            .with_context(|| {
                format!(
                    "executing chown to change ownership of symlink at {}",
                    link.display()
                )
            })?;
        anyhow::ensure!(
            res.status.success(),
            "failed to change ownership of symlink at {}\n\nSTDERR: {}",
            link.display(),
            String::from_utf8_lossy(&res.stderr),
        );
        Ok(())
    }
}

#[derive(Copy, Clone)]
struct Target<'a> {
    path: &'a Path,
    uid: u32,
    gid: u32,
}

async fn create_symlinks_for(
    component: &str,
    tedge: Target<'_>,
    executable_dir: &Path,
    fs: &(impl FileSystem + std::marker::Sync),
) -> anyhow::Result<()> {
    let link = executable_dir.join(component);
    match fs.read_link(&link).await {
        Err(e) if e.kind() != ErrorKind::NotFound => bail!(
            "couldn't read metadata for {}. do you need to run with sudo?",
            link.display()
        ),
        existing_file => {
            if let Ok(target) = existing_file {
                // If the symlink already exists, don't modify it
                if target == tedge.path {
                    debug!("Leaving symlink for {component} unchanged");
                    return Ok(());
                }

                fs.unlink(&link).await.with_context(|| {
                    format!("removing old version of {component} at {}", link.display())
                })?;
            }

            fs.symlink(tedge.path, &link).await.with_context(|| {
                format!(
                    "creating symlink for {component} to {}",
                    tedge.path.display()
                )
            })?;

            fs.chown_symlink(&link, tedge.uid, tedge.gid).await?;
            Ok(())
        }
    }
}

pub struct RealEnv;
impl FileSystem for RealEnv {}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;

    mod create_symlinks_for {
        use super::*;

        #[tokio::test]
        async fn replaces_binaries_with_symlinks() {
            let mut fs = MockFileSystem::new();
            // Simulate a non-symlinked file - read link returns the input path
            fs.expect_read_link().return_once(|input| Ok(input.into()));
            fs.expect_unlink()
                .with(eq(Path::new("/usr/bin/tedge-mapper")))
                .times(1)
                .returning(|_| Ok(()));
            fs.expect_symlink()
                .with(
                    eq(Path::new("/usr/bin/tedge")),
                    eq(Path::new("/usr/bin/tedge-mapper")),
                )
                .times(1)
                .returning(|_, _| Ok(()));
            fs.expect_chown_symlink()
                .times(1)
                .with(eq(Path::new("/usr/bin/tedge-mapper")), eq(987), eq(986))
                .returning(|_, _, _| Ok(()));
            let target = Target {
                path: Path::new("/usr/bin/tedge"),
                uid: 987,
                gid: 986,
            };

            create_symlinks_for("tedge-mapper", target, Path::new("/usr/bin"), &fs)
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn creates_symlinks_if_they_dont_exist() {
            let mut fs = MockFileSystem::new();
            // Simulate a non-symlinked file - read link returns the input path
            fs.expect_read_link().return_once(|_| io_error::not_found());
            fs.expect_symlink()
                .with(
                    eq(Path::new("/usr/bin/tedge")),
                    eq(Path::new("/usr/bin/tedge-mapper")),
                )
                .times(1)
                .returning(|_, _| Ok(()));
            fs.expect_chown_symlink()
                .times(1)
                .with(eq(Path::new("/usr/bin/tedge-mapper")), eq(987), eq(986))
                .returning(|_, _, _| Ok(()));
            let target = Target {
                path: Path::new("/usr/bin/tedge"),
                uid: 987,
                gid: 986,
            };

            create_symlinks_for("tedge-mapper", target, Path::new("/usr/bin"), &fs)
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn replaces_symlinks_if_they_differ_from_the_configured_target() {
            let mut fs = MockFileSystem::new();
            // Simulate a non-symlinked file - read link returns the input path
            fs.expect_read_link()
                .return_once(|_| Ok("/usr/bin/tedge".into()));
            fs.expect_unlink()
                .with(eq(Path::new("/usr/bin/tedge-mapper")))
                .times(1)
                .returning(|_| Ok(()));
            fs.expect_symlink()
                .with(
                    eq(Path::new("tedge")),
                    eq(Path::new("/usr/bin/tedge-mapper")),
                )
                .times(1)
                .returning(|_, _| Ok(()));
            fs.expect_chown_symlink()
                .times(1)
                .with(eq(Path::new("/usr/bin/tedge-mapper")), eq(987), eq(986))
                .returning(|_, _, _| Ok(()));
            let target = Target {
                path: Path::new("tedge"),
                uid: 987,
                gid: 986,
            };

            create_symlinks_for("tedge-mapper", target, Path::new("/usr/bin"), &fs)
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn leaves_up_to_date_symlinks_unchanged() {
            let mut fs = MockFileSystem::new();
            // Simulate a non-symlinked file - read link returns the input path
            fs.expect_read_link()
                .return_once(|_| Ok("/usr/bin/tedge".into()));
            let target = Target {
                path: Path::new("/usr/bin/tedge"),
                uid: 987,
                gid: 986,
            };

            create_symlinks_for("tedge-mapper", target, Path::new("/usr/bin"), &fs)
                .await
                .unwrap()
        }
    }

    #[derive(thiserror::Error, Debug)]
    #[error("{0}")]
    struct DummyError(&'static str);

    mod io_error {
        use crate::cli::init::tests::DummyError;
        use std::io::ErrorKind;

        pub fn not_found<T>() -> std::io::Result<T> {
            Err(std::io::Error::new(
                ErrorKind::NotFound,
                Box::new(DummyError("File not found")),
            ))
        }
    }
}
