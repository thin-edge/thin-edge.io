use crate::command::BuildContext;
use crate::command::Command;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DataPathSetting;
use tedge_config::LogPathSetting;
use tedge_utils::file::create_directory;

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
        create_directory(&config_dir, Some(self.user.clone()), None, Some(0o775))?;

        create_directory(
            config_dir.join("mosquitto-conf"),
            Some("mosquitto".into()),
            Some("mosquitto".into()),
            Some(0o775),
        )?;
        create_directory(
            config_dir.join("operations"),
            Some(self.user.clone()),
            Some(self.group.clone()),
            Some(0o775),
        )?;
        create_directory(
            config_dir.join("plugins"),
            Some(self.user.clone()),
            Some(self.group.clone()),
            Some(0o775),
        )?;
        create_directory(
            config_dir.join("device-certs"),
            Some("mosquitto".into()),
            Some("mosquitto".into()),
            Some(0o775),
        )?;

        let config = self.context.config_repository.load()?;

        let log_dir = config.query(LogPathSetting)?;
        create_directory(
            log_dir.join("tedge"),
            Some(self.user.clone()),
            Some(self.group.clone()),
            Some(0o775),
        )?;

        let data_dir = config.query(DataPathSetting)?;
        create_directory(
            data_dir,
            Some(self.user.clone()),
            Some(self.group.clone()),
            Some(0o775),
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
    }
}
