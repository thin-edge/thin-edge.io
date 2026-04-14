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
use tedge_utils::file;
use tedge_utils::file::create_directory_and_update_ownership_with_root;
use tedge_utils::paths::TedgePaths;
use tracing::debug;
use tracing::info;

pub struct TEdgeInitCmd {
    user: Option<String>,
    group: Option<String>,
    relative_links: bool,
}

impl TEdgeInitCmd {
    pub fn new(user: Option<String>, group: Option<String>, relative_links: bool) -> Self {
        Self {
            user,
            group,
            relative_links,
        }
    }
}

impl TEdgeInitCmd {
    async fn initialize_tedge(&self, config: TEdgeConfig) -> anyhow::Result<()> {
        let system_config = config.read_system_config();

        let user = Self::resolve_config_value("User", self.user.clone(), system_config.user);
        let group = Self::resolve_config_value("Group", self.group.clone(), system_config.group);

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
        let config_root = TedgePaths::from_root_with_defaults(config.root_dir(), &user, &group);
        let permissions = file::permissions(&user, &group, 0o775);
        for dir in config_root_directories() {
            config_root.dir(dir)?.with_mode(0o775).ensure().await?;
        }

        create_directory_and_update_ownership_with_root(
            &config.logs.path,
            &config.logs.path,
            &permissions,
        )
        .await?;
        create_directory_and_update_ownership_with_root(
            &config.data.path,
            &config.data.path,
            &permissions,
        )
        .await?;

        let file_permissions = file::permissions(&user, &group, 0o644);
        let system_toml = config_dir.join("system.toml");
        if system_toml.exists() {
            file_permissions
                .clone()
                .apply(system_toml.as_std_path())
                .await?;
        }

        let agent_state_dir = if config.agent.state.path.exists() {
            config.agent.state.path.to_path_buf()
        } else {
            let agent_state_dir = config_dir.join(".agent");
            config_root.dir(".agent")?.with_mode(0o775).ensure().await?;
            agent_state_dir
        };

        let entity_store_file = agent_state_dir.join("entity_store.jsonl");
        if entity_store_file.exists() {
            file_permissions
                .apply(entity_store_file.as_std_path())
                .await?;
        }

        Ok(())
    }

    fn resolve_config_value(
        field_name: &str,
        cli_value: Option<String>,
        system_value: String,
    ) -> String {
        match cli_value {
            Some(val) => {
                info!("{} '{}' received from CLI arguments", field_name, val);
                val
            }
            None => {
                info!(
                    "{} '{}' received from system.toml",
                    field_name, system_value
                );
                system_value
            }
        }
    }
}

fn config_root_directories() -> [&'static str; 7] {
    [
        "mosquitto-conf",
        "operations/c8y",
        "plugins",
        "sm-plugins",
        "device-certs",
        "mappers",
        ".tedge-mapper-c8y",
    ]
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

    mod init {
        use super::*;
        use tedge_config::TEdgeConfig;
        use tedge_test_utils::fs::TempTedgeDir;
        use uzers::get_group_by_gid;

        fn current_user_and_group() -> (String, String) {
            let user = whoami::username();
            let gid = nix::unistd::getgid().as_raw();
            let group = get_group_by_gid(gid)
                .expect("group must exist")
                .name()
                .to_string_lossy()
                .into_owned();

            (user, group)
        }

        #[tokio::test]
        async fn initializes_directories() {
            let ttd = TempTedgeDir::new();
            let tedge_dir = ttd.dir("tedge");
            let logs_dir = tedge_dir.utf8_path().join("logs");
            let data_dir = tedge_dir.utf8_path().join("data");
            tedge_dir.file("tedge.toml").with_raw_content(&format!(
                "logs.path = \"{logs_dir}\"\ndata.path = \"{data_dir}\"\n",
            ));
            let config = TEdgeConfig::load(tedge_dir.path()).await.unwrap();
            let (user, group) = current_user_and_group();
            TEdgeInitCmd::new(Some(user), Some(group), false)
                .execute(config)
                .await
                .unwrap();

            for dir in config_root_directories() {
                assert!(
                    tedge_dir.path().join(dir).exists(),
                    "config directory {dir} should be created"
                );
            }
            for dir in [logs_dir, data_dir] {
                assert!(dir.exists(), "directory {dir} should be created");
            }
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

    mod resolve_config_value {
        use super::*;

        #[test]
        fn uses_system_toml_value_when_no_cli_value_is_given() {
            assert_eq!(
                TEdgeInitCmd::resolve_config_value("User", None, "system-user".into()),
                "system-user"
            );
        }

        #[test]
        fn cli_value_overrides_system_toml_value() {
            assert_eq!(
                TEdgeInitCmd::resolve_config_value(
                    "User",
                    Some("cli-user".into()),
                    "system-user".into()
                ),
                "cli-user"
            );
        }
    }
}
