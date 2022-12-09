use crate::{
    error::AgentError,
    http_rest,
    restart_operation_handler::restart_operation,
    state::{
        AgentStateRepository, RestartOperationStatus, SoftwareOperationVariants, State,
        StateRepository, StateStatus,
    },
};
use flockfile::{check_another_instance_is_not_running, Flockfile};
use tedge_api::{
    control_filter_topic, software_filter_topic, Jsonify, OperationStatus, RestartOperationRequest,
    RestartOperationResponse, SoftwareError, SoftwareListRequest, SoftwareListResponse,
    SoftwareRequestResponse, SoftwareType, SoftwareUpdateRequest, SoftwareUpdateResponse,
};

use mqtt_channel::{Connection, Message, PubChannel, StreamExt, SubChannel, Topic, TopicFilter};
use plugin_sm::{
    operation_logs::{LogKind, OperationLogs},
    plugin_manager::{ExternalPlugins, Plugins},
};

use crate::http_rest::HttpConfig;
use std::process::Command;
use std::{convert::TryInto, fmt::Debug, path::PathBuf, sync::Arc};
use tedge_api::health::{health_check_topics, send_health_status};
use tedge_config::{
    system_services::SystemConfig, ConfigRepository, ConfigSettingAccessor,
    ConfigSettingAccessorStringExt, HttpBindAddressSetting, HttpPortSetting, LogPathSetting,
    MqttBindAddressSetting, MqttPortSetting, RunPathSetting, SoftwarePluginDefaultSetting,
    TEdgeConfigLocation, TmpPathSetting, DEFAULT_LOG_PATH, DEFAULT_RUN_PATH, DEFAULT_TMP_PATH,
};
use tedge_utils::file::create_directory_with_user_group;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, warn};

use std::path::Path;

const SYNC: &str = "sync";
const SM_PLUGINS: &str = "sm-plugins";
const AGENT_LOG_PATH: &str = "tedge/agent";

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
    pub sm_home: PathBuf,
    pub log_dir: PathBuf,
    pub run_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub config_location: TEdgeConfigLocation,
    pub download_dir: PathBuf,
    pub http_config: HttpConfig,
}

impl Default for SmAgentConfig {
    fn default() -> Self {
        let errors_topic = Topic::new("tedge/errors").expect("Invalid topic");

        let mqtt_config = mqtt_channel::Config::default();

        let mut request_topics: TopicFilter = vec![software_filter_topic(), control_filter_topic()]
            .try_into()
            .expect("Invalid topic filter");

        let request_topics_health: TopicFilter = health_check_topics("tedge-agent");

        request_topics.add_all(request_topics_health.clone());

        let response_topic_health = Topic::new_unchecked("tedge/health/tedge-agent");

        let request_topic_list =
            Topic::new(SoftwareListRequest::topic_name()).expect("Invalid topic");

        let request_topic_update =
            Topic::new(SoftwareUpdateRequest::topic_name()).expect("Invalid topic");

        let response_topic_list =
            Topic::new(SoftwareListResponse::topic_name()).expect("Invalid topic");

        let response_topic_update =
            Topic::new(SoftwareUpdateResponse::topic_name()).expect("Invalid topic");

        let request_topic_restart =
            Topic::new(RestartOperationRequest::topic_name()).expect("Invalid topic");

        let response_topic_restart =
            Topic::new(RestartOperationResponse::topic_name()).expect("Invalid topic");

        let sm_home = PathBuf::from("/etc/tedge");

        let log_dir = PathBuf::from(&format!("{DEFAULT_LOG_PATH}/{AGENT_LOG_PATH}"));

        let run_dir = PathBuf::from(DEFAULT_RUN_PATH);

        let tmp_dir = PathBuf::from(DEFAULT_TMP_PATH);

        let config_location = TEdgeConfigLocation::default();

        let download_dir = PathBuf::from(DEFAULT_TMP_PATH);

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
            config_location,
            download_dir,
            http_config: HttpConfig::default(),
        }
    }
}

impl SmAgentConfig {
    pub fn try_new(tedge_config_location: TEdgeConfigLocation) -> Result<Self, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let mqtt_config = mqtt_channel::Config::default()
            .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
            .with_port(tedge_config.query(MqttPortSetting)?.into())
            .with_max_packet_size(10 * 1024 * 1024);

        let tedge_config_path = config_repository
            .get_config_location()
            .tedge_config_root_path()
            .to_path_buf();

        let tedge_download_dir = tedge_config.query_string(TmpPathSetting)?.into();

        let tedge_log_dir: String = tedge_config.query_string(LogPathSetting)?;
        let tedge_log_dir = PathBuf::from(&format!("{tedge_log_dir}/{AGENT_LOG_PATH}"));
        let tedge_run_dir = tedge_config.query_string(RunPathSetting)?.into();
        let tedge_tmp_dir = tedge_config.query_string(TmpPathSetting)?.into();

        let mut http_config = HttpConfig::default();

        let http_bind_address = tedge_config.query(HttpBindAddressSetting)?;
        http_config = http_config
            .with_port(tedge_config.query(HttpPortSetting)?.0)
            .with_ip_address(http_bind_address.into());

        Ok(SmAgentConfig::default()
            .with_sm_home(tedge_config_path)
            .with_mqtt_config(mqtt_config)
            .with_config_location(tedge_config_location)
            .with_download_directory(tedge_download_dir)
            .with_log_directory(tedge_log_dir)
            .with_run_directory(tedge_run_dir)
            .with_tmp_directory(tedge_tmp_dir)
            .with_http_config(http_config))
    }

    pub fn with_sm_home(self, sm_home: PathBuf) -> Self {
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

    pub fn with_download_directory(self, tmp_dir: PathBuf) -> Self {
        Self {
            download_dir: tmp_dir,
            ..self
        }
    }

    pub fn with_log_directory(self, log_dir: PathBuf) -> Self {
        Self { log_dir, ..self }
    }

    pub fn with_run_directory(self, run_dir: PathBuf) -> Self {
        Self { run_dir, ..self }
    }

    pub fn with_tmp_directory(self, tmp_dir: PathBuf) -> Self {
        Self { tmp_dir, ..self }
    }

    pub fn with_http_config(self, http_config: HttpConfig) -> Self {
        Self {
            http_config,
            ..self
        }
    }
}

#[derive(Debug)]
pub struct SmAgent {
    config: SmAgentConfig,
    operation_logs: OperationLogs,
    persistence_store: AgentStateRepository,
    _flock: Flockfile,
}

impl SmAgent {
    pub fn try_new(name: &str, mut config: SmAgentConfig) -> Result<Self, AgentError> {
        let flock = check_another_instance_is_not_running(name, &config.run_dir)?;
        info!("{} starting", &name);

        let persistence_store = AgentStateRepository::new(config.sm_home.clone());
        let operation_logs = OperationLogs::try_new(config.log_dir.clone())?;

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
    pub async fn init(&mut self, config_dir: PathBuf) -> Result<(), anyhow::Error> {
        // `config_dir` by default is `/etc/tedge` (or whatever the user sets with --config-dir)
        let config_dir = config_dir.display();
        create_directory_with_user_group(&format!("{config_dir}/.agent"), "tedge", "tedge", 0o775)?;
        create_directory_with_user_group(self.config.log_dir.clone(), "tedge", "tedge", 0o775)?;
        create_directory_with_user_group(
            &self.config.http_config.file_transfer_dir_as_string(),
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
    pub async fn start(&mut self) -> Result<(), AgentError> {
        info!("Starting tedge agent");

        let mut mqtt = Connection::new(&self.config.mqtt_config).await?;
        let sm_plugins_path = self.config.sm_home.join(SM_PLUGINS);

        let plugins = Arc::new(Mutex::new(ExternalPlugins::open(
            &sm_plugins_path,
            get_default_plugin(&self.config.config_location)?,
            Some(SUDO.into()),
        )?));

        if plugins.lock().await.empty() {
            warn!(
                "{}",
                AgentError::NoPlugins {
                    plugins_path: sm_plugins_path,
                }
            );
        }

        let mut mqtt_errors = mqtt.errors;
        tokio::spawn(async move {
            while let Some(error) = mqtt_errors.next().await {
                error!("{}", error);
            }
        });

        self.process_pending_operation(&mut mqtt.published).await?;

        let http_config = self.config.http_config.clone();

        // spawning file transfer server
        tokio::spawn(async move {
            start_http_file_transfer_server(&http_config).await;
        });

        while let Err(error) = self
            .process_subscribed_messages(&mut mqtt.received, &mut mqtt.published, &plugins)
            .await
        {
            error!("{}", error);
        }
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

                topic if topic == &self.config.request_topic_restart => {
                    let request = self
                        .match_restart_operation_payload(responses, &message)
                        .await?;
                    if let Err(error) = self
                        .handle_restart_operation(responses, &self.config.response_topic_restart)
                        .await
                    {
                        error!("{}", error);

                        self.persistence_store.clear().await?;
                        let status = OperationStatus::Failed;
                        let response = RestartOperationResponse::new(&request).with_status(status);
                        responses
                            .publish(Message::new(
                                &self.config.response_topic_restart,
                                response.to_bytes()?,
                            ))
                            .await?;
                    }
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
                    .process(&request, log_file, &self.config.download_dir)
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

    async fn handle_restart_operation(
        &self,
        responses: &mut impl PubChannel,
        topic: &Topic,
    ) -> Result<(), AgentError> {
        self.persistence_store
            .update(&StateStatus::Restart(RestartOperationStatus::Restarting))
            .await?;

        // update status to executing.
        let executing_response = RestartOperationResponse::new(&RestartOperationRequest::default());
        responses
            .publish(Message::new(topic, executing_response.to_bytes()?))
            .await?;
        restart_operation::create_tmp_restart_file(&self.config.tmp_dir)?;

        let command_vec =
            get_restart_operation_commands(&self.config.config_location.tedge_config_root_path)?;
        for mut command in command_vec {
            match command.status() {
                Ok(status) => {
                    if !status.success() {
                        return Err(AgentError::CommandFailed);
                    }
                }
                Err(e) => {
                    return Err(AgentError::FromIo(e));
                }
            }
        }

        Ok(())
    }

    async fn process_pending_operation(
        &self,
        responses: &mut impl PubChannel,
    ) -> Result<(), AgentError> {
        let state: Result<State, _> = self.persistence_store.load().await;
        let mut status = OperationStatus::Failed;

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

                StateStatus::Restart(RestartOperationStatus::Pending) => {
                    &self.config.response_topic_restart
                }

                StateStatus::Restart(RestartOperationStatus::Restarting) => {
                    let _state = self.persistence_store.clear().await?;
                    if restart_operation::has_rebooted(&self.config.tmp_dir)? {
                        info!("Device restart successful.");
                        status = OperationStatus::Successful;
                    }
                    &self.config.response_topic_restart
                }

                StateStatus::UnknownOperation => {
                    error!("UnknownOperation in store.");
                    &self.config.errors_topic
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

async fn start_http_file_transfer_server(http_config: &HttpConfig) {
    let server = http_rest::http_file_transfer_server(http_config);

    match server {
        Ok(server) => {
            if let Err(err) = server.await {
                error!("{}", err);
            }
        }
        Err(err) => error!("{}", err),
    }
}

fn get_restart_operation_commands(system_config_path: &Path) -> Result<Vec<Command>, AgentError> {
    let mut vec = vec![];
    // sync first
    let mut sync_command = std::process::Command::new(SUDO);
    sync_command.arg(SYNC);
    vec.push(sync_command);

    // reading `system_config_path` to get the restart command or defaulting to `["init", "6"]'
    let system_config = SystemConfig::try_new(system_config_path.to_path_buf())?;

    let mut command = std::process::Command::new(SUDO);
    command.args(system_config.system.reboot);
    vec.push(command);
    Ok(vec)
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

    use std::path::PathBuf;

    use assert_json_diff::assert_json_include;
    use serde_json::{json, Value};

    use super::*;

    use tedge_test_utils::fs::TempTedgeDir;

    const TEDGE_AGENT_RESTART: &str = "tedge_agent_restart";

    #[tokio::test]
    async fn check_agent_restart_file_is_created() -> Result<(), AgentError> {
        let (dir, tedge_config_location) = create_temp_tedge_config().unwrap();
        let agent = SmAgent::try_new(
            "tedge_agent_test",
            SmAgentConfig::try_new(tedge_config_location).unwrap(),
        )
        .unwrap();

        // calling handle_restart_operation should create a file in /tmp/tedge_agent_restart
        let (_output, mut output_stream) = mqtt_tests::output_stream();
        let response_topic_restart =
            Topic::new(RestartOperationResponse::topic_name()).expect("Invalid topic");

        agent
            .handle_restart_operation(&mut output_stream, &response_topic_restart)
            .await?;

        assert!(
            std::path::Path::new(&dir.temp_dir.path().join("tmp").join(TEDGE_AGENT_RESTART))
                .exists()
        );

        Ok(())
    }

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
        ttd.dir("run").dir("tedge_agent");
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

            let response_topic_restart =
                Topic::new(SoftwareListResponse::topic_name()).expect("Invalid topic");

            let plugins = Arc::new(Mutex::new(
                ExternalPlugins::open(
                    PathBuf::from(&dir.temp_dir.path()).join("sm-plugins"),
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
                    PathBuf::from(&dir.temp_dir.path()).join("sm-plugins"),
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
        let handle_one = tokio::spawn(async move {
            start_http_file_transfer_server(&config_clone).await;
        });

        let handle_two = tokio::spawn(async move {
            start_http_file_transfer_server(&http_config).await;
        });

        // although the code inside handle_two throws an error it does not panic.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // to check for the error, we assert that handle_one is still running
        // while handle_two is finished.
        assert_eq!(handle_one.is_finished(), false);
        assert_eq!(handle_two.is_finished(), true);

        Ok(())
    }
}
