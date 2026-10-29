use tedge_utils::paths::ManagedDir;

const SHIPPED_WORKFLOWS: [(&str, &str); 1] =
    [("control.toml", include_str!("../resources/control.toml"))];

pub async fn install_service_workflows(ops_dir: &ManagedDir) -> Result<(), anyhow::Error> {
    for (file_name, definition) in SHIPPED_WORKFLOWS {
        ops_dir
            .template_file(file_name)?
            .persist(definition)
            .await?;
    }

    Ok(())
}
