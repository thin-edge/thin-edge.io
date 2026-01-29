use crate::error::ConfigManagementError;
use crate::plugin_manager::parse_config_type;
use crate::plugin_manager::ExternalPlugins;
use crate::ConfigManagerConfig;
use crate::ConfigSetRequest;
use crate::ConfigSetResponse;
use async_trait::async_trait;
use camino::Utf8Path;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

#[derive(Clone)]
pub struct ConfigPluginServer {
    external_plugins: ExternalPlugins,
}

impl ConfigPluginServer {
    pub fn new(config: ConfigManagerConfig) -> Self {
        let external_plugins = ExternalPlugins::new(
            config.plugin_dirs.clone(),
            config.sudo_enabled,
            config.tmp_path.clone(),
        );
        Self { external_plugins }
    }

    pub fn builder(self) -> ServerActorBuilder<ConfigPluginServer, Sequential> {
        ServerActorBuilder::new(self, &ServerConfig::default(), Sequential)
    }
}

#[async_trait]
impl Server for ConfigPluginServer {
    type Request = ConfigSetRequest;
    type Response = ConfigSetResponse;

    fn name(&self) -> &str {
        "ConfigPluginServer"
    }

    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        match self.set_config(&request).await {
            Ok(()) => ConfigSetResponse::Success,
            Err(err) => ConfigSetResponse::Error(err.to_string()),
        }
    }
}

impl ConfigPluginServer {
    async fn set_config(
        &mut self,
        request: &ConfigSetRequest,
    ) -> Result<(), ConfigManagementError> {
        let from_path = Utf8Path::new(&request.downloaded_path);

        if !from_path.exists() {
            return Err(ConfigManagementError::FileNotFound(
                request.downloaded_path.clone(),
            ));
        }

        let (config_type, plugin_type) = parse_config_type(&request.config_type);
        let plugin = self
            .external_plugins
            .by_plugin_type(plugin_type)
            .ok_or_else(|| ConfigManagementError::PluginNotFound(plugin_type.to_string()))?;

        plugin.set(config_type, from_path, None).await?;

        Ok(())
    }
}
