use crate::error::AgentError;
use crate::file_transfer_server::actor::FileTransferServerBuilder;
use crate::file_transfer_server::http_rest::HttpConfig;
use crate::mqtt_operation_converter::builder::MqttOperationConverterBuilder;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use crate::software_list_manager::builder::SoftwareListManagerBuilder;
use crate::software_list_manager::config::SoftwareListManagerConfig;
use crate::software_update_manager::builder::SoftwareUpdateManagerBuilder;
use crate::software_update_manager::config::SoftwareUpdateManagerConfig;
use camino::Utf8PathBuf;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use std::fmt::Debug;
use tedge_actors::Runtime;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;

use tedge_config::DataPathSetting;
use tedge_config::Flag;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::LockFilesSetting;
use tedge_config::LogPathSetting;
use tedge_config::RunPathSetting;
use tedge_config::TEdgeConfigError;
use tedge_config::TEdgeConfigLocation;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_utils::file::create_directory_with_user_group;
use tracing::info;
use tracing::instrument;

const TEDGE_AGENT: &str = "tedge-agent";

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub mqtt_config: MqttConfig,
    pub http_config: HttpConfig,
    pub restart_config: RestartManagerConfig,
    pub sw_list_config: SoftwareListManagerConfig,
    pub sw_update_config: SoftwareUpdateManagerConfig,
    pub run_dir: Utf8PathBuf,
    pub use_lock: Flag,
    pub log_dir: Utf8PathBuf,
    pub data_dir: Utf8PathBuf,
}

impl AgentConfig {
    pub fn from_tedge_config(
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<Self, TEdgeConfigError> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_max_packet_size(10 * 1024 * 1024)
            .with_session_name(TEDGE_AGENT);

        // HTTP config
        let data_dir = tedge_config.query(DataPathSetting)?;
        let http_bind_address = tedge_config.query(HttpBindAddressSetting)?;
        let http_port = tedge_config.query(HttpPortSetting)?.0;
        let http_config = HttpConfig::default()
            .with_data_dir(data_dir.clone())
            .with_port(http_port)
            .with_ip_address(http_bind_address.into());

        // Restart config
        let restart_config = RestartManagerConfig::from_tedge_config(tedge_config_location)?;

        // Software list config
        let sw_list_config = SoftwareListManagerConfig::from_tedge_config(tedge_config_location)?;

        // Software update config
        let sw_update_config =
            SoftwareUpdateManagerConfig::from_tedge_config(tedge_config_location)?;

        // For flockfile
        let run_dir = tedge_config.query(RunPathSetting)?;
        let use_lock = tedge_config.query(LockFilesSetting)?;

        // For agent specific
        let log_dir = tedge_config
            .query(LogPathSetting)?
            .join("tedge")
            .join("agent");

        Ok(Self {
            mqtt_config,
            http_config,
            restart_config,
            sw_list_config,
            sw_update_config,
            run_dir,
            use_lock,
            data_dir,
            log_dir,
        })
    }
}

#[derive(Debug)]
pub struct Agent {
    config: AgentConfig,
    _flock: Option<Flockfile>,
}

impl Agent {
    pub fn try_new(name: &str, config: AgentConfig) -> Result<Self, AgentError> {
        let mut flock = None;
        if config.use_lock.is_set() {
            flock = Some(check_another_instance_is_not_running(
                name,
                config.run_dir.as_std_path(),
            )?);
        }

        info!("{} starting", &name);

        Ok(Self {
            config,
            _flock: flock,
        })
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn init(&mut self, config_dir: Utf8PathBuf) -> Result<(), anyhow::Error> {
        // `config_dir` by default is `/etc/tedge` (or whatever the user sets with --config-dir)
        create_directory_with_user_group(format!("{config_dir}/.agent"), "tedge", "tedge", 0o775)?;
        create_directory_with_user_group(self.config.log_dir.clone(), "tedge", "tedge", 0o775)?;
        create_directory_with_user_group(self.config.data_dir.clone(), "tedge", "tedge", 0o775)?;
        create_directory_with_user_group(
            self.config.http_config.file_transfer_dir_as_string(),
            "tedge",
            "tedge",
            0o775,
        )?;

        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(&mut self) -> Result<(), anyhow::Error> {
        info!("Starting tedge agent");

        // Runtime
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        // File transfer server actor
        let file_transfer_server_builder =
            FileTransferServerBuilder::new(self.config.http_config.clone());

        // Restart actor
        let mut restart_actor_builder =
            RestartManagerBuilder::new(self.config.restart_config.clone());

        // Mqtt actor
        let mut mqtt_actor_builder = MqttActorBuilder::new(
            self.config
                .mqtt_config
                .clone()
                .with_session_name(TEDGE_AGENT),
        );

        // Software list actor
        let mut software_list_builder =
            SoftwareListManagerBuilder::new(self.config.sw_list_config.clone());

        // Software update actor
        let mut software_update_builder =
            SoftwareUpdateManagerBuilder::new(self.config.sw_update_config.clone());

        // Converter actor
        let converter_actor_builder = MqttOperationConverterBuilder::new(
            &mut software_list_builder,
            &mut software_update_builder,
            &mut restart_actor_builder,
            &mut mqtt_actor_builder,
        );

        // Shutdown on SIGINT
        let signal_actor_builder = SignalActor::builder(&runtime.get_handle());

        // Health actor
        let health_actor = HealthMonitorBuilder::new(TEDGE_AGENT, &mut mqtt_actor_builder);

        // Spawn all
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(file_transfer_server_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(restart_actor_builder).await?;
        runtime.spawn(software_list_builder).await?;
        runtime.spawn(software_update_builder).await?;
        runtime.spawn(converter_actor_builder).await?;
        runtime.spawn(health_actor).await?;

        runtime.run_to_completion().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // use assert_json_diff::assert_json_include;
    // use serde_json::json;
    // use serde_json::Value;
    //
    // use super::*;
    //
    // use tedge_test_utils::fs::TempTedgeDir;

    const TEDGE_AGENT_RESTART: &str = "tedge_agent_restart";

    // #[tokio::test]
    // async fn check_agent_restart_file_is_created() -> Result<(), AgentError> {
    //     let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();
    //     let agent = SmAgent::try_new(
    //         "tedge_agent_test",
    //         SmAgentConfig::try_new(tedge_config_location).unwrap(),
    //     )
    //     .unwrap();
    //
    //     // calling handle_restart_operation should create a file in /tmp/tedge_agent_restart
    //     let (_output, mut output_stream) = mqtt_tests::output_stream();
    //     let response_topic_restart = RestartOperationResponse::topic();
    //
    //     agent
    //         .handle_restart_operation(&mut output_stream, &response_topic_restart)
    //         .await?;
    //
    //     assert!(
    //         std::path::Path::new(&dir.temp_dir.path().join("tmp").join(TEDGE_AGENT_RESTART))
    //             .exists()
    //     );
    //
    //     Ok(())
    // }

    // fn message(t: &str, p: &str) -> Message {
    //     let topic = Topic::new(t).expect("a valid topic");
    //     let payload = p.as_bytes();
    //     Message::new(&topic, payload)
    // }
    //
    // fn create_temp_tedge_config() -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
    //     let ttd = TempTedgeDir::new();
    //     ttd.dir(".agent").file("current-operation");
    //     ttd.dir("sm-plugins");
    //     ttd.dir("tmp");
    //     ttd.dir("logs");
    //     ttd.dir("run").dir("tedge-agent");
    //     ttd.dir("run").dir("lock");
    //     let system_toml_content = toml::toml! {
    //         [system]
    //         reboot = ["echo", "6"]
    //     };
    //     ttd.file("system.toml")
    //         .with_toml_content(system_toml_content);
    //     let toml_conf = &format!(
    //         r#"
    //         [tmp]
    //         path = '{}'
    //         [logs]
    //         path = '{}'
    //         [run]
    //         path = '{}'"#,
    //         &ttd.temp_dir.path().join("tmp").to_str().unwrap(),
    //         &ttd.temp_dir.path().join("logs").to_str().unwrap(),
    //         &ttd.temp_dir.path().join("run").to_str().unwrap()
    //     );
    //     ttd.file("tedge.toml").with_raw_content(toml_conf);
    //
    //     let config_location = TEdgeConfigLocation::from_custom_root(ttd.temp_dir.path());
    //     Ok((ttd, config_location))
    // }
    //
    // #[tokio::test]
    // /// testing that tedge agent returns an expety software list when there is no sm plugin
    // async fn test_empty_software_list_returned_when_no_sm_plugin() -> Result<(), AgentError> {
    //     let (output, mut output_sink) = mqtt_tests::output_stream();
    //     let expected_messages = vec![
    //         message(
    //             r#"tedge/commands/res/software/list"#,
    //             r#"{"id":"123","status":"executing"}"#,
    //         ),
    //         message(
    //             r#"tedge/commands/res/software/list"#,
    //             r#"{"id":"123","status":"successful","currentSoftwareList":[{"type":"","modules":[]}]}"#,
    //         ),
    //     ];
    //     let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();
    //
    //     tokio::spawn(async move {
    //         let agent = SmAgent::try_new(
    //             "tedge_agent_test",
    //             SmAgentConfig::try_new(tedge_config_location).unwrap(),
    //         )
    //         .unwrap();
    //
    //         let response_topic_restart = SoftwareListResponse::topic();
    //
    //         let plugins = Arc::new(Mutex::new(
    //             ExternalPlugins::open(
    //                 dir.utf8_path().join("sm-plugins"),
    //                 get_default_plugin(&agent.config.config_location).unwrap(),
    //                 Some(SUDO.into()),
    //             )
    //             .unwrap(),
    //         ));
    //         agent
    //             .handle_software_list_request(
    //                 &mut output_sink,
    //                 plugins,
    //                 &response_topic_restart,
    //                 &Message::new(&response_topic_restart, r#"{"id":"123"}"#),
    //             )
    //             .await
    //             .unwrap();
    //     });
    //
    //     let response = output.collect().await;
    //     assert_eq!(expected_messages, response);
    //
    //     Ok(())
    // }
    //
    // #[tokio::test]
    // /// test health check request response contract
    // async fn health_check() -> Result<(), AgentError> {
    //     let (responses, mut response_sink) = mqtt_tests::output_stream();
    //     let mut requests = mqtt_tests::input_stream(vec![
    //         message("tedge/health-check/tedge-agent", ""),
    //         message("tedge/health-check", ""),
    //     ])
    //     .await;
    //
    //     let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();
    //
    //     tokio::spawn(async move {
    //         let mut agent = SmAgent::try_new(
    //             "tedge_agent_test",
    //             SmAgentConfig::try_new(tedge_config_location).unwrap(),
    //         )
    //         .unwrap();
    //
    //         let plugins = Arc::new(Mutex::new(
    //             ExternalPlugins::open(
    //                 dir.utf8_path().join("sm-plugins"),
    //                 get_default_plugin(&agent.config.config_location).unwrap(),
    //                 Some(SUDO.into()),
    //             )
    //             .unwrap(),
    //         ));
    //         agent
    //             .process_subscribed_messages(&mut requests, &mut response_sink, &plugins)
    //             .await
    //             .unwrap();
    //     });
    //
    //     let responses = responses.collect().await;
    //     assert_eq!(responses.len(), 2);
    //
    //     for response in responses {
    //         assert_eq!(response.topic.name, "tedge/health/tedge-agent");
    //         let health_status: Value = serde_json::from_slice(response.payload_bytes())?;
    //         assert_json_include!(actual: &health_status, expected: json!({"status": "up"}));
    //         assert!(health_status["pid"].is_number());
    //     }
    //
    //     Ok(())
    // }
    //
    // #[tokio::test]
    // #[serial_test::serial]
    // async fn check_tedge_agent_does_not_panic_when_port_is_in_use() -> Result<(), anyhow::Error> {
    //     let http_config = HttpConfig::default().with_port(3000);
    //     let config_clone = http_config.clone();
    //
    //     // handle_one uses port 3000.
    //     // handle_two will not be able to bind to the same port.
    //     let http_server_builder = HttpServerBuilder::new(http_config);
    //     let mut http_server_actor = http_server_builder.build();
    //     let http_server_builder_two = HttpServerBuilder::new(config_clone);
    //     let mut http_server_actor_two = http_server_builder_two.build();
    //
    //     let handle_one = tokio::spawn(async move {
    //         http_server_actor.run().await.unwrap();
    //     });
    //
    //     let handle_two = tokio::spawn(async move {
    //         http_server_actor_two.run().await.unwrap();
    //     });
    //
    //     // although the code inside handle_two throws an error it does not panic.
    //     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    //
    //     // to check for the error, we assert that handle_one is still running
    //     // while handle_two is finished.
    //     assert!(!handle_one.is_finished());
    //     assert!(handle_two.is_finished());
    //
    //     Ok(())
    // }
}
