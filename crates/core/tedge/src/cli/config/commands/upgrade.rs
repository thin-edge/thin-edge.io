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
        tedge_config.migrate_mapper_configs().await.map_err(|e| {
            MaybeFancy::Unfancy(anyhow::Error::new(e).context(
                "Failed to migrate mapper configurations. \
                     Fix the underlying issue and run 'tedge config upgrade' again to retry.",
            ))
        })?;

        println!("Configuration updates completed successfully.");
        Ok(())
    }
}
