use tedge_utils::paths::ManagedDir;

/// Using an array to expand later for other actions: start, stop, enable, disable
const SHIPPED_WORKFLOWS: [(&str, &str); 1] = [(
    "service_restart.toml",
    include_str!("../resources/service_restart.toml"),
)];

pub async fn install_service_workflows(ops_dir: &ManagedDir) -> Result<(), anyhow::Error> {
    for (file_name, definition) in SHIPPED_WORKFLOWS {
        ops_dir
            .template_file(file_name)?
            .persist(definition)
            .await?;
    }

    Ok(())
}
