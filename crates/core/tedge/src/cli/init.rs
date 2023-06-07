use crate::command::BuildContext;
use crate::command::Command;
use anyhow::Context;
use tedge_utils::file::create_directory;
use tedge_utils::file::PermissionEntry;

#[derive(Debug)]
pub struct TEdgeInitCmd {
    user: String,
    group: String,
    context: BuildContext,
}

impl TEdgeInitCmd {
    pub fn new(user: String, group: String, context: BuildContext) -> Self {
        Self {
            user,
            group,
            context,
        }
    }

    fn initialize_tedge(&self) -> anyhow::Result<()> {
        let config_dir = self.context.config_location.tedge_config_root_path.clone();
        create_directory(
            &config_dir,
            PermissionEntry::new(Some(self.user.clone()), None, Some(0o775)),
        )?;

        create_directory(
            config_dir.join("mosquitto-conf"),
            PermissionEntry::new(
                Some("mosquitto".into()),
                Some("mosquitto".into()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("operations"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("plugins"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;
        create_directory(
            config_dir.join("device-certs"),
            PermissionEntry::new(
                Some("mosquitto".into()),
                Some("mosquitto".into()),
                Some(0o775),
            ),
        )?;

        let config = self.context.config_repository.load_new()?;

        create_directory(
            config.logs.path.join("tedge"),
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        create_directory(
            &config.data.path,
            PermissionEntry::new(
                Some(self.user.clone()),
                Some(self.group.clone()),
                Some(0o775),
            ),
        )?;

        Ok(())
    }
}

impl Command for TEdgeInitCmd {
    fn description(&self) -> String {
        "Initialize tedge".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.initialize_tedge()
            .with_context(|| "Failed to initialize tedge. You have to run tedge with sudo.")
    }
}
