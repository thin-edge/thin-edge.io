use crate::error::AgentError;
use crate::http_server::actor::HttpServerBuilder;
use crate::http_server::http_rest::HttpConfig;
use crate::mqtt_operation_converter::builder::MqttOperationConverterBuilder;
use crate::restart_manager::actor::RestartManagerConfig;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::state_repository::state::AgentStateRepository;
use crate::state_repository::state::RestartOperationStatus;
use crate::state_repository::state::SoftwareOperationVariants;
use crate::state_repository::state::State;
use crate::state_repository::state::StateRepository;
use crate::state_repository::state::StateStatus;
use camino::Utf8PathBuf;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::SubChannel;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use plugin_sm::operation_logs::LogKind;
use plugin_sm::operation_logs::OperationLogs;
use plugin_sm::plugin_manager::ExternalPlugins;
use plugin_sm::plugin_manager::Plugins;
use std::convert::TryInto;
use std::fmt::Debug;
use std::sync::Arc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::Runtime;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_api::health::send_health_status;
use tedge_api::software_filter_topic;
use tedge_api::Jsonify;
use tedge_api::OperationStatus;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_api::SoftwareError;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareRequestResponse;
use tedge_api::SoftwareType;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::ConfigSettingAccessorStringExt;
use tedge_config::DataPathSetting;
use tedge_config::Flag;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::LockFilesSetting;
use tedge_config::LogPathSetting;
use tedge_config::RunPathSetting;
use tedge_config::SoftwarePluginDefaultSetting;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TmpPathSetting;
use tedge_config::DEFAULT_DATA_PATH;
use tedge_config::DEFAULT_LOG_PATH;
use tedge_config::DEFAULT_RUN_PATH;
use tedge_config::DEFAULT_TMP_PATH;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_utils::file::create_directory_with_user_group;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::instrument;
use tracing::warn;

const SYNC: &str = "sync";
const SM_PLUGINS: &str = "sm-plugins";
const AGENT_LOG_PATH: &str = "tedge/agent";
const TEDGE_AGENT: &str = "tedge-agent";

#[cfg(not(test))]
const SUDO: &str = "sudo";

#[cfg(test)]
const SUDO: &str = "echo";

#[derive(Debug, Clone)]
pub struct SmAgentConfig {
    pub errors_topic: Topic,
    pub mqtt_config: mqtt_channel::Config,
    pub request_topics_health: TopicFilter,
    pub request_topic_list: Topic,
    pub request_topic_update: Topic,
    pub request_topics: TopicFilter,
    pub request_topic_restart: Topic,
    pub response_topic_health: Topic,
    pub response_topic_list: Topic,
    pub response_topic_update: Topic,
    pub response_topic_restart: Topic,
    pub sm_home: Utf8PathBuf,
    pub log_dir: Utf8PathBuf,
    pub run_dir: Utf8PathBuf,
    pub tmp_dir: Utf8PathBuf,
    pub data_dir: Utf8PathBuf,
    pub config_location: TEdgeConfigLocation,
    pub download_dir: Utf8PathBuf,
    pub http_config: HttpConfig,
    pub use_lock: Flag,
}

impl Default for SmAgentConfig {
    fn default() -> Self {
        let errors_topic = Topic::new("tedge/errors").expect("Invalid topic");

        let mqtt_config = mqtt_channel::Config::default();

        let mut request_topics: TopicFilter = vec![software_filter_topic()]
            .try_into()
            .expect("Invalid topic filter");

        let request_topics_health: TopicFilter = health_check_topics("tedge-agent");

        request_topics.add_all(request_topics_health.clone());

        let response_topic_health = Topic::new_unchecked("tedge/health/tedge-agent");

        let request_topic_list = SoftwareListRequest::topic();

        let request_topic_update = SoftwareUpdateRequest::topic();

        let response_topic_list = SoftwareListResponse::topic();

        let response_topic_update = SoftwareUpdateResponse::topic();

        let request_topic_restart = RestartOperationRequest::topic();

        let response_topic_restart = RestartOperationResponse::topic();

        let sm_home = Utf8PathBuf::from("/etc/tedge");

        let log_dir = Utf8PathBuf::from(&format!("{DEFAULT_LOG_PATH}/{AGENT_LOG_PATH}"));

        let run_dir = Utf8PathBuf::from(DEFAULT_RUN_PATH);

        let tmp_dir = Utf8PathBuf::from(DEFAULT_TMP_PATH);

        let config_location = TEdgeConfigLocation::default();

        let download_dir = Utf8PathBuf::from(DEFAULT_TMP_PATH);

        let use_lock = Flag(true);

        let data_dir = Utf8PathBuf::from(DEFAULT_DATA_PATH);

        let http_config = HttpConfig::default().with_data_dir(data_dir.clone());

        Self {
            errors_topic,
            mqtt_config,
            request_topics_health,
            request_topic_list,
            request_topic_update,
            request_topics,
            response_topic_health,
            response_topic_list,
            response_topic_update,
            request_topic_restart,
            response_topic_restart,
            sm_home,
            log_dir,
            run_dir,
            tmp_dir,
            data_dir,
            config_location,
            download_dir,
            http_config,
            use_lock,
        }
    }
}

impl SmAgentConfig {
    pub fn try_new(tedge_config_location: TEdgeConfigLocation) -> Result<Self, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_max_packet_size(10 * 1024 * 1024)
            .with_session_name(TEDGE_AGENT)
            .with_initial_message(|| health_status_up_message(TEDGE_AGENT))
            .with_last_will_message(health_status_down_message(TEDGE_AGENT));

        let tedge_config_path = config_repository
            .get_config_location()
            .tedge_config_root_path()
            .to_path_buf();

        let tedge_download_dir = tedge_config.query_string(TmpPathSetting)?.into();

        let tedge_log_dir: String = tedge_config.query_string(LogPathSetting)?;
        let tedge_log_dir = Utf8PathBuf::from(&format!("{tedge_log_dir}/{AGENT_LOG_PATH}"));
        let tedge_run_dir = tedge_config.query_string(RunPathSetting)?.into();
        let tedge_tmp_dir = tedge_config.query_string(TmpPathSetting)?.into();
        let tedge_data_dir = tedge_config.query(DataPathSetting)?;

        let mut http_config = HttpConfig::default().with_data_dir(tedge_data_dir.clone());

        let http_bind_address = tedge_config.query(HttpBindAddressSetting)?;
        http_config = http_config
            .with_port(tedge_config.query(HttpPortSetting)?.0)
            .with_ip_address(http_bind_address.into());

        let use_lock = tedge_config.query(LockFilesSetting)?;

        Ok(SmAgentConfig::default()
            .with_sm_home(tedge_config_path)
            .with_mqtt_config(mqtt_config)
            .with_config_location(tedge_config_location)
            .with_download_directory(tedge_download_dir)
            .with_log_directory(tedge_log_dir)
            .with_run_directory(tedge_run_dir)
            .with_tmp_directory(tedge_tmp_dir)
            .with_data_directory(tedge_data_dir)
            .with_http_config(http_config)
            .with_use_lock(use_lock))
    }

    pub fn with_sm_home(self, sm_home: Utf8PathBuf) -> Self {
        Self { sm_home, ..self }
    }

    pub fn with_mqtt_config(self, mqtt_config: mqtt_channel::Config) -> Self {
        Self {
            mqtt_config,
            ..self
        }
    }

    pub fn with_config_location(self, config_location: TEdgeConfigLocation) -> Self {
        Self {
            config_location,
            ..self
        }
    }

    pub fn with_download_directory(self, tmp_dir: Utf8PathBuf) -> Self {
        Self {
            download_dir: tmp_dir,
            ..self
        }
    }

    pub fn with_log_directory(self, log_dir: Utf8PathBuf) -> Self {
        Self { log_dir, ..self }
    }

    pub fn with_run_directory(self, run_dir: Utf8PathBuf) -> Self {
        Self { run_dir, ..self }
    }

    pub fn with_tmp_directory(self, tmp_dir: Utf8PathBuf) -> Self {
        Self { tmp_dir, ..self }
    }

    pub fn with_data_directory(self, data_dir: Utf8PathBuf) -> Self {
        Self { data_dir, ..self }
    }

    pub fn with_http_config(self, http_config: HttpConfig) -> Self {
        Self {
            http_config,
            ..self
        }
    }

    pub fn with_use_lock(self, use_lock: Flag) -> Self {
        Self { use_lock, ..self }
    }
}

#[derive(Debug)]
pub struct SmAgent {
    config: SmAgentConfig,
    operation_logs: OperationLogs,
    persistence_store: AgentStateRepository,
    _flock: Option<Flockfile>,
}

impl SmAgent {
    pub fn try_new(name: &str, mut config: SmAgentConfig) -> Result<Self, AgentError> {
        let mut flock = None;
        if config.use_lock.is_set() {
            flock = Some(check_another_instance_is_not_running(
                name,
                config.run_dir.as_std_path(),
            )?);
        }

        info!("{} starting", &name);

        let persistence_store = AgentStateRepository::new(config.sm_home.clone());
        let operation_logs = OperationLogs::try_new(config.log_dir.clone().into())?;

        config.mqtt_config = config
            .mqtt_config
            .with_session_name(name)
            .with_subscriptions(config.request_topics.clone());

        Ok(Self {
            config,
            operation_logs,
            persistence_store,
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
        info!("Initializing the tedge agent session");
        mqtt_channel::init_session(&self.config.mqtt_config).await?;

        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn clear_session(&mut self) -> Result<(), AgentError> {
        info!("Cleaning the tedge agent session");
        mqtt_channel::clear_session(&self.config.mqtt_config).await?;
        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(&mut self) -> Result<(), anyhow::Error> {
        info!("Starting tedge agent");

        // let mut mqtt = Connection::new(&self.config.mqtt_config).await?;
        // let sm_plugins_path = self.config.sm_home.join(SM_PLUGINS);
        //
        // let plugins = Arc::new(Mutex::new(ExternalPlugins::open(
        //     &sm_plugins_path,
        //     get_default_plugin(&self.config.config_location)?,
        //     Some(SUDO.into()),
        // )?));
        //
        // if plugins.lock().await.empty() {
        //     warn!(
        //         "{}",
        //         AgentError::NoPlugins {
        //             plugins_path: sm_plugins_path,
        //         }
        //     );
        // }
        //
        // let mut mqtt_errors = mqtt.errors;
        // tokio::spawn(async move {
        //     while let Some(error) = mqtt_errors.next().await {
        //         error!("{}", error);
        //     }
        // });
        //
        // self.process_pending_operation(&mut mqtt.published).await?;
        //
        // send_health_status(&mut mqtt.published, TEDGE_AGENT).await;

        // Actor stuff
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        // File transfer server actor
        let http_config = self.config.http_config.clone();
        let mut http_server_builder = HttpServerBuilder::new(http_config);

        // Restart actor
        let restart_config = RestartManagerConfig::new(
            self.config.tmp_dir.clone(),
            self.config.sm_home.clone(),
            self.config.config_location.tedge_config_root_path.clone(),
        );
        let mut restart_actor_builder = RestartManagerBuilder::new(restart_config);

        // Mqtt actor
        let mut mqtt_actor_builder = MqttActorBuilder::new(
            self.config
                .mqtt_config
                .clone()
                .with_session_name(TEDGE_AGENT),
        );

        // TBD
        let mut software_list_builder: SimpleMessageBoxBuilder<
            SoftwareListRequest,
            SoftwareListResponse,
        > = SimpleMessageBoxBuilder::new("SoftwareList", 5);
        let mut software_update_builder: SimpleMessageBoxBuilder<
            SoftwareUpdateRequest,
            SoftwareUpdateResponse,
        > = SimpleMessageBoxBuilder::new("SoftwareUpdate", 5);
        // Converter actor
        let converter_actor_builder = MqttOperationConverterBuilder::new(
            &mut software_list_builder,
            &mut software_update_builder,
            &mut restart_actor_builder,
            &mut mqtt_actor_builder,
        );

        // Shutdown on SIGINT
        let signal_actor_builder = SignalActor::builder(&runtime.get_handle());

        // Spawn
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(http_server_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(restart_actor_builder).await?;
        // runtime.spawn(software_list_builder).await?;
        // runtime.spawn(software_update_builder).await?;
        runtime.spawn(converter_actor_builder).await?;

        runtime.run_to_completion().await?;

        // while let Err(error) = self
        //     .process_subscribed_messages(&mut mqtt.received, &mut mqtt.published, &plugins)
        //     .await
        // {
        //     error!("{}", error);
        // }
        Ok(())
    }

    async fn process_subscribed_messages(
        &mut self,
        requests: &mut impl SubChannel,
        responses: &mut impl PubChannel,
        plugins: &Arc<Mutex<ExternalPlugins>>,
    ) -> Result<(), AgentError> {
        while let Some(message) = requests.next().await {
            debug!("Request {:?}", message);
            match &message.topic {
                topic if self.config.request_topics_health.accept_topic(topic) => {
                    send_health_status(responses, "tedge-agent").await;
                }

                topic if topic == &self.config.request_topic_list => {
                    let _success = self
                        .handle_software_list_request(
                            responses,
                            plugins.clone(),
                            &self.config.response_topic_list,
                            &message,
                        )
                        .await
                        .map_err(|err| {
                            error!("{:?}", err); // log error and discard such that the agent doesn't exit.
                        });
                }

                topic if topic == &self.config.request_topic_update => {
                    plugins.lock().await.load()?;
                    plugins
                        .lock()
                        .await
                        .update_default(&get_default_plugin(&self.config.config_location)?)?;

                    let _success = self
                        .handle_software_update_request(
                            responses,
                            plugins.clone(),
                            &self.config.response_topic_update,
                            &message,
                        )
                        .await
                        .map_err(|err| {
                            error!("{:?}", err); // log error and discard such that the agent doesn't exit.
                        });
                }

                _ => error!("Unknown operation. Discarded."),
            }
        }

        Ok(())
    }

    async fn handle_software_list_request(
        &self,
        responses: &mut impl PubChannel,
        plugins: Arc<Mutex<ExternalPlugins>>,
        response_topic: &Topic,
        message: &Message,
    ) -> Result<(), AgentError> {
        let request = match SoftwareListRequest::from_slice(message.payload_bytes()) {
            Ok(request) => {
                self.persistence_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
                    })
                    .await?;

                request
            }

            Err(error) => {
                debug!("Parsing error: {}", error);
                responses
                    .publish(Message::new(
                        &self.config.errors_topic,
                        format!("{}", error),
                    ))
                    .await?;

                return Err(SoftwareError::ParseError {
                    reason: "Parsing Error".into(),
                }
                .into());
            }
        };
        let mut executing_response = SoftwareListResponse::new(&request);

        responses
            .publish(Message::new(
                &self.config.response_topic_list,
                executing_response.to_bytes()?,
            ))
            .await?;

        let response = match self
            .operation_logs
            .new_log_file(LogKind::SoftwareList)
            .await
        {
            Ok(log_file) => plugins.lock().await.list(&request, log_file).await,
            Err(err) => {
                error!("{}", err);
                executing_response.set_error(&format!("{}", err));
                executing_response
            }
        };

        responses
            .publish(Message::new(response_topic, response.to_bytes()?))
            .await?;

        let _state: State = self.persistence_store.clear().await?;

        Ok(())
    }

    async fn handle_software_update_request(
        &self,
        responses: &mut impl PubChannel,
        plugins: Arc<Mutex<ExternalPlugins>>,
        response_topic: &Topic,
        message: &Message,
    ) -> Result<(), AgentError> {
        let request = match SoftwareUpdateRequest::from_slice(message.payload_bytes()) {
            Ok(request) => {
                let _ = self
                    .persistence_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Software(SoftwareOperationVariants::Update)),
                    })
                    .await;

                request
            }

            Err(error) => {
                error!("Parsing error: {}", error);
                responses
                    .publish(Message::new(
                        &self.config.errors_topic,
                        format!("{}", error),
                    ))
                    .await?;

                return Err(SoftwareError::ParseError {
                    reason: "Parsing failed".into(),
                }
                .into());
            }
        };

        let mut executing_response = SoftwareUpdateResponse::new(&request);
        responses
            .publish(Message::new(response_topic, executing_response.to_bytes()?))
            .await?;

        let response = match self
            .operation_logs
            .new_log_file(LogKind::SoftwareUpdate)
            .await
        {
            Ok(log_file) => {
                plugins
                    .lock()
                    .await
                    .process(&request, log_file, self.config.download_dir.as_std_path())
                    .await
            }
            Err(err) => {
                error!("{}", err);
                executing_response.set_error(&format!("{}", err));
                executing_response
            }
        };

        responses
            .publish(Message::new(response_topic, response.to_bytes()?))
            .await?;

        let _state = self.persistence_store.clear().await?;

        Ok(())
    }

    async fn match_restart_operation_payload(
        &self,
        responses: &mut impl PubChannel,
        message: &Message,
    ) -> Result<RestartOperationRequest, AgentError> {
        let request = match RestartOperationRequest::from_slice(message.payload_bytes()) {
            Ok(request) => {
                self.persistence_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Restart(RestartOperationStatus::Restarting)),
                    })
                    .await?;
                request
            }

            Err(error) => {
                error!("Parsing error: {}", error);
                responses
                    .publish(Message::new(
                        &self.config.errors_topic,
                        format!("{}", error),
                    ))
                    .await?;

                return Err(SoftwareError::ParseError {
                    reason: "Parsing failed".into(),
                }
                .into());
            }
        };
        Ok(request)
    }

    async fn process_pending_operation(
        &self,
        responses: &mut impl PubChannel,
    ) -> Result<(), AgentError> {
        let state: Result<State, _> = self.persistence_store.load().await;
        let status = OperationStatus::Failed;

        if let State {
            operation_id: Some(id),
            operation: Some(operation),
        } = match state {
            Ok(state) => state,
            Err(_) => State {
                operation_id: None,
                operation: None,
            },
        } {
            let topic = match operation {
                StateStatus::Software(SoftwareOperationVariants::List) => {
                    &self.config.response_topic_list
                }

                StateStatus::Software(SoftwareOperationVariants::Update) => {
                    &self.config.response_topic_update
                }

                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                    &self.config.errors_topic
                }
                _ => {
                    unimplemented!()
                }
            };

            let response = SoftwareRequestResponse::new(&id, status);

            responses
                .publish(Message::new(topic, response.to_bytes()?))
                .await?;
        }

        Ok(())
    }
}

fn get_default_plugin(
    config_location: &TEdgeConfigLocation,
) -> Result<Option<SoftwareType>, AgentError> {
    let config_repository = tedge_config::TEdgeConfigRepository::new(config_location.clone());
    let tedge_config = config_repository.load()?;

    Ok(tedge_config.query_string_optional(SoftwarePluginDefaultSetting)?)
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_include;
    use serde_json::json;
    use serde_json::Value;

    use super::*;

    use tedge_test_utils::fs::TempTedgeDir;

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

    fn message(t: &str, p: &str) -> Message {
        let topic = Topic::new(t).expect("a valid topic");
        let payload = p.as_bytes();
        Message::new(&topic, payload)
    }

    fn create_temp_tedge_config() -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
        let ttd = TempTedgeDir::new();
        ttd.dir(".agent").file("current-operation");
        ttd.dir("sm-plugins");
        ttd.dir("tmp");
        ttd.dir("logs");
        ttd.dir("run").dir("tedge-agent");
        ttd.dir("run").dir("lock");
        let system_toml_content = toml::toml! {
            [system]
            reboot = ["echo", "6"]
        };
        ttd.file("system.toml")
            .with_toml_content(system_toml_content);
        let toml_conf = &format!(
            r#"
            [tmp]
            path = '{}'
            [logs]
            path = '{}'
            [run]
            path = '{}'"#,
            &ttd.temp_dir.path().join("tmp").to_str().unwrap(),
            &ttd.temp_dir.path().join("logs").to_str().unwrap(),
            &ttd.temp_dir.path().join("run").to_str().unwrap()
        );
        ttd.file("tedge.toml").with_raw_content(toml_conf);

        let config_location = TEdgeConfigLocation::from_custom_root(ttd.temp_dir.path());
        Ok((ttd, config_location))
    }

    #[tokio::test]
    /// testing that tedge agent returns an expety software list when there is no sm plugin
    async fn test_empty_software_list_returned_when_no_sm_plugin() -> Result<(), AgentError> {
        let (output, mut output_sink) = mqtt_tests::output_stream();
        let expected_messages = vec![
            message(
                r#"tedge/commands/res/software/list"#,
                r#"{"id":"123","status":"executing"}"#,
            ),
            message(
                r#"tedge/commands/res/software/list"#,
                r#"{"id":"123","status":"successful","currentSoftwareList":[{"type":"","modules":[]}]}"#,
            ),
        ];
        let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();

        tokio::spawn(async move {
            let agent = SmAgent::try_new(
                "tedge_agent_test",
                SmAgentConfig::try_new(tedge_config_location).unwrap(),
            )
            .unwrap();

            let response_topic_restart = SoftwareListResponse::topic();

            let plugins = Arc::new(Mutex::new(
                ExternalPlugins::open(
                    dir.utf8_path().join("sm-plugins"),
                    get_default_plugin(&agent.config.config_location).unwrap(),
                    Some(SUDO.into()),
                )
                .unwrap(),
            ));
            agent
                .handle_software_list_request(
                    &mut output_sink,
                    plugins,
                    &response_topic_restart,
                    &Message::new(&response_topic_restart, r#"{"id":"123"}"#),
                )
                .await
                .unwrap();
        });

        let response = output.collect().await;
        assert_eq!(expected_messages, response);

        Ok(())
    }

    #[tokio::test]
    /// test health check request response contract
    async fn health_check() -> Result<(), AgentError> {
        let (responses, mut response_sink) = mqtt_tests::output_stream();
        let mut requests = mqtt_tests::input_stream(vec![
            message("tedge/health-check/tedge-agent", ""),
            message("tedge/health-check", ""),
        ])
        .await;

        let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();

        tokio::spawn(async move {
            let mut agent = SmAgent::try_new(
                "tedge_agent_test",
                SmAgentConfig::try_new(tedge_config_location).unwrap(),
            )
            .unwrap();

            let plugins = Arc::new(Mutex::new(
                ExternalPlugins::open(
                    dir.utf8_path().join("sm-plugins"),
                    get_default_plugin(&agent.config.config_location).unwrap(),
                    Some(SUDO.into()),
                )
                .unwrap(),
            ));
            agent
                .process_subscribed_messages(&mut requests, &mut response_sink, &plugins)
                .await
                .unwrap();
        });

        let responses = responses.collect().await;
        assert_eq!(responses.len(), 2);

        for response in responses {
            assert_eq!(response.topic.name, "tedge/health/tedge-agent");
            let health_status: Value = serde_json::from_slice(response.payload_bytes())?;
            assert_json_include!(actual: &health_status, expected: json!({"status": "up"}));
            assert!(health_status["pid"].is_number());
        }

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn check_tedge_agent_does_not_panic_when_port_is_in_use() -> Result<(), anyhow::Error> {
        let http_config = HttpConfig::default().with_port(3000);
        let config_clone = http_config.clone();

        // handle_one uses port 3000.
        // handle_two will not be able to bind to the same port.
        let http_server_builder = HttpServerBuilder::new(http_config);
        let mut http_server_actor = http_server_builder.build();
        let http_server_builder_two = HttpServerBuilder::new(config_clone);
        let mut http_server_actor_two = http_server_builder_two.build();

        let handle_one = tokio::spawn(async move {
            http_server_actor.run().await.unwrap();
        });

        let handle_two = tokio::spawn(async move {
            http_server_actor_two.run().await.unwrap();
        });

        // although the code inside handle_two throws an error it does not panic.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // to check for the error, we assert that handle_one is still running
        // while handle_two is finished.
        assert!(!handle_one.is_finished());
        assert!(handle_two.is_finished());

        Ok(())
    }
}
