use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::TEdgeConfig;

pub struct UpgradeConfigCommand;

#[async_trait::async_trait]
impl Command for UpgradeConfigCommand {
    fn description(&self) -> String {
        "upgrade the configuration format".to_owned()
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let backup_path = tedge_config.migrate_mapper_configs().await.map_err(|e| {
            MaybeFancy::Unfancy(anyhow::Error::new(e).context(
                "Failed to migrate mapper configurations. \
                     Fix the underlying issue and run 'tedge config upgrade' again to retry.",
            ))
        })?;

        eprintln!("Configuration updates completed successfully.");
        eprintln!(
            "Your original configuration has been backed up to: {} in case the old configuration must be restored.",
            backup_path
        );
        eprintln!(
            "You may delete it after validating that your upgraded configuration is working."
        );
        Ok(())
    }
}
