use crate::{
    error::AgentError,
    restart_operation_handler::restart_operation,
    state::{
        AgentStateRepository, RestartOperationStatus, SoftwareOperationVariants, State,
        StateRepository, StateStatus,
    },
};
use flockfile::{check_another_instance_is_not_running, Flockfile};

use mqtt_client::{Client, Config, Message, MqttClient, MqttMessageStream, Topic, TopicFilter};
use plugin_sm::plugin_manager::{ExternalPlugins, Plugins};
use sm_interface::{
    control_filter_topic, software_filter_topic, Jsonify, OperationStatus, RestartOperationRequest,
    RestartOperationResponse, SoftwareError, SoftwareListRequest, SoftwareListResponse,
    SoftwareRequestResponse, SoftwareType, SoftwareUpdateRequest, SoftwareUpdateResponse,
};
use std::{
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, instrument};

use crate::operation_logs::{LogKind, OperationLogs};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, ConfigSettingAccessorStringExt, MqttPortSetting,
    SoftwarePluginDefaultSetting, TEdgeConfigLocation,
};

#[cfg(not(test))]
const INIT_COMMAND: &'static str = "init";

#[cfg(test)]
const INIT_COMMAND: &'static str = "echo";

#[derive(Debug)]
pub struct SmAgentConfig {
    pub errors_topic: Topic,
    pub mqtt_client_config: mqtt_client::Config,
    pub request_topic_list: Topic,
    pub request_topic_update: Topic,
    pub request_topics: TopicFilter,
    pub request_topic_restart: Topic,
    pub response_topic_list: Topic,
    pub response_topic_update: Topic,
    pub response_topic_restart: Topic,
    pub sm_home: PathBuf,
    pub log_dir: PathBuf,
    config_location: TEdgeConfigLocation,
}

impl Default for SmAgentConfig {
    fn default() -> Self {
        let errors_topic = Topic::new("tedge/errors").expect("Invalid topic");

        let mqtt_client_config = mqtt_client::Config::default().with_packet_size(10 * 1024 * 1024);

        let mut request_topics = TopicFilter::new(software_filter_topic()).expect("Invalid topic");
        let () = request_topics
            .add(control_filter_topic())
            .expect("Invalid topic filter");

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

        let log_dir = PathBuf::from("/var/log/tedge/agent");

        let config_location = TEdgeConfigLocation::from_default_system_location();

        Self {
            errors_topic,
            mqtt_client_config,
            request_topic_list,
            request_topic_update,
            request_topics,
            response_topic_list,
            response_topic_update,
            request_topic_restart,
            response_topic_restart,
            sm_home,
            log_dir,
            config_location,
        }
    }
}

impl SmAgentConfig {
    pub fn try_new(tedge_config_location: TEdgeConfigLocation) -> Result<Self, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let mqtt_config =
            mqtt_client::Config::default().with_port(tedge_config.query(MqttPortSetting)?.into());

        let tedge_config_path = config_repository
            .get_config_location()
            .tedge_config_root_path()
            .to_path_buf();

        Ok(SmAgentConfig::default()
            .with_sm_home(tedge_config_path)
            .with_mqtt_client_config(mqtt_config)
            .with_config_location(tedge_config_location))
    }

    pub fn with_sm_home(self, sm_home: PathBuf) -> Self {
        Self { sm_home, ..self }
    }

    pub fn with_mqtt_client_config(self, mqtt_client_config: Config) -> Self {
        Self {
            mqtt_client_config,
            ..self
        }
    }

    pub fn with_config_location(self, config_location: TEdgeConfigLocation) -> Self {
        Self {
            config_location,
            ..self
        }
    }
}

#[derive(Debug)]
pub struct SmAgent {
    config: SmAgentConfig,
    name: String,
    operation_logs: OperationLogs,
    persistance_store: AgentStateRepository,
    _flock: Flockfile,
}

impl SmAgent {
    pub fn try_new(name: &str, config: SmAgentConfig) -> Result<Self, AgentError> {
        let flock = check_another_instance_is_not_running(name)?;
        info!("{} starting", &name);

        let persistance_store = AgentStateRepository::new(config.sm_home.clone());
        let operation_logs = OperationLogs::try_new(config.log_dir.clone())?;

        Ok(Self {
            config,
            name: name.into(),
            operation_logs,
            persistance_store,
            _flock: flock,
        })
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(&mut self) -> Result<(), AgentError> {
        info!("Starting tedge agent");

        let mqtt = Client::connect(self.name.as_str(), &self.config.mqtt_client_config).await?;
        let mut operations = mqtt.subscribe(self.config.request_topics.clone()).await?;

        let plugins = Arc::new(Mutex::new(ExternalPlugins::open(
            self.config.sm_home.join("sm-plugins"),
            get_default_plugin(&self.config.config_location)?,
            Some("sudo".into()),
        )?));

        if plugins.lock().unwrap().empty() {
            // `unwrap` should be safe here as we only access data.
            error!("Couldn't load plugins from /etc/tedge/sm-plugins");
            return Err(AgentError::NoPlugins);
        }

        let mut errors = mqtt.subscribe_errors();
        tokio::spawn(async move {
            while let Some(error) = errors.next().await {
                error!("{}", error);
            }
        });

        let () = self.process_pending_operation(&mqtt).await?;

        // * Maybe it would be nice if mapper/registry responds
        let () = publish_capabilities(&mqtt).await?;
        while let Err(error) = self
            .process_subscribed_messages(&mqtt, &mut operations, &plugins)
            .await
        {
            error!("{}", error);
        }

        Ok(())
    }

    async fn process_subscribed_messages(
        &mut self,
        mqtt: &Client,
        operations: &mut Box<dyn MqttMessageStream>,
        plugins: &Arc<Mutex<ExternalPlugins>>,
    ) -> Result<(), AgentError> {
        while let Some(message) = operations.next().await {
            debug!("Request {:?}", message);
            match &message.topic {
                topic if topic == &self.config.request_topic_list => {
                    let _success = self
                        .handle_software_list_request(
                            mqtt,
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
                    let () = plugins.lock().unwrap().load()?; // `unwrap` should be safe here as we only access data for write.
                    let () = plugins
                        .lock()
                        .unwrap() // `unwrap` should be safe here as we only access data for write.
                        .update_default(&get_default_plugin(&self.config.config_location)?)?;

                    let _success = self
                        .handle_software_update_request(
                            mqtt,
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
                    let request = self.match_restart_operation_payload(mqtt, &message).await?;
                    if let Err(error) = self
                        .handle_restart_operation(mqtt, &self.config.response_topic_restart)
                        .await
                    {
                        error!("{}", error);

                        self.persistance_store.clear().await?;
                        let status = OperationStatus::Failed;
                        let response = RestartOperationResponse::new(&request).with_status(status);
                        let () = mqtt
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
        mqtt: &Client,
        plugins: Arc<Mutex<ExternalPlugins>>,
        response_topic: &Topic,
        message: &Message,
    ) -> Result<(), AgentError> {
        let request = match SoftwareListRequest::from_slice(message.payload_trimmed()) {
            Ok(request) => {
                let () = self
                    .persistance_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Software(SoftwareOperationVariants::List)),
                    })
                    .await?;

                request
            }

            Err(error) => {
                debug!("Parsing error: {}", error);
                let _ = mqtt
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
        let executing_response = SoftwareListResponse::new(&request);

        let _ = mqtt
            .publish(Message::new(
                &self.config.response_topic_list,
                executing_response.to_bytes()?,
            ))
            .await?;

        let log_file = self
            .operation_logs
            .new_log_file(LogKind::SoftwareList)
            .await?;
        let response = plugins.lock().unwrap().list(&request, log_file).await; // `unwrap` should be safe here as we only access data.

        let _ = mqtt
            .publish(Message::new(response_topic, response.to_bytes()?))
            .await?;

        let _state: State = self.persistance_store.clear().await?;

        Ok(())
    }

    async fn handle_software_update_request(
        &self,
        mqtt: &Client,
        plugins: Arc<Mutex<ExternalPlugins>>,
        response_topic: &Topic,
        message: &Message,
    ) -> Result<(), AgentError> {
        let request = match SoftwareUpdateRequest::from_slice(message.payload_trimmed()) {
            Ok(request) => {
                let () = self
                    .persistance_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Software(SoftwareOperationVariants::Update)),
                    })
                    .await?;

                request
            }

            Err(error) => {
                error!("Parsing error: {}", error);
                let _ = mqtt
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

        let executing_response = SoftwareUpdateResponse::new(&request);
        let _ = mqtt
            .publish(Message::new(response_topic, executing_response.to_bytes()?))
            .await?;

        let log_file = self
            .operation_logs
            .new_log_file(LogKind::SoftwareUpdate)
            .await?;

        let response = plugins.lock().unwrap().process(&request, log_file).await; // `unwrap` should be safe here as we only access data.

        let _ = mqtt
            .publish(Message::new(response_topic, response.to_bytes()?))
            .await?;

        let _state = self.persistance_store.clear().await?;

        Ok(())
    }

    async fn match_restart_operation_payload(
        &self,
        mqtt: &Client,
        message: &Message,
    ) -> Result<RestartOperationRequest, AgentError> {
        let request = match RestartOperationRequest::from_slice(message.payload_trimmed()) {
            Ok(request) => {
                let () = self
                    .persistance_store
                    .store(&State {
                        operation_id: Some(request.id.clone()),
                        operation: Some(StateStatus::Restart(RestartOperationStatus::Restarting)),
                    })
                    .await?;
                request
            }

            Err(error) => {
                error!("Parsing error: {}", error);
                let _ = mqtt
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
        mqtt: &Client,
        topic: &Topic,
    ) -> Result<(), AgentError> {
        self.persistance_store
            .update(&StateStatus::Restart(RestartOperationStatus::Restarting))
            .await?;

        // update status to executing.
        let executing_response = RestartOperationResponse::new(&RestartOperationRequest::new());
        let _ = mqtt
            .publish(Message::new(&topic, executing_response.to_bytes()?))
            .await?;
        let () = restart_operation::create_slash_run_file()?;

        let _process_result = std::process::Command::new("sudo").arg("sync").status();
        // state = "Restarting"
        match std::process::Command::new("sudo")
            .arg(INIT_COMMAND)
            .arg("6")
            .status()
        {
            Ok(process_status) => {
                if !process_status.success() {
                    return Err(AgentError::CommandFailed);
                }
            }
            Err(e) => {
                return Err(AgentError::FromIo(e));
            }
        }

        Ok(())
    }

    async fn process_pending_operation(&self, mqtt: &Client) -> Result<(), AgentError> {
        let state: Result<State, _> = self.persistance_store.load().await;
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
                    let _state = self.persistance_store.clear().await?;
                    if restart_operation::has_rebooted()? {
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

            let () = mqtt
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

async fn publish_capabilities(mqtt: &Client) -> Result<(), AgentError> {
    mqtt.publish(Message::new(&Topic::new("tedge/capabilities/software/list")?, "").retain())
        .await?;

    mqtt.publish(Message::new(&Topic::new("tedge/capabilities/software/update")?, "").retain())
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SLASH_RUN_PATH_TEDGE_AGENT_RESTART: &str = "/run/tedge_agent/tedge_agent_restart";

    #[ignore]
    #[tokio::test]
    async fn check_agent_restart_file_is_created() -> Result<(), AgentError> {
        assert_eq!(INIT_COMMAND, "echo");
        let tedge_config_location =
            tedge_config::TEdgeConfigLocation::from_default_system_location();
        let agent = SmAgent::try_new(
            "tedge_agent_test",
            SmAgentConfig::try_new(tedge_config_location).unwrap(),
        )
        .unwrap();

        // calling handle_restart_operation should create a file in /run/tedge_agent_restart
        let mqtt = Client::connect(
            "sm-agent-test",
            &mqtt_client::Config::default().with_packet_size(10 * 1024 * 1024),
        )
        .await?;
        let response_topic_restart =
            Topic::new(RestartOperationResponse::topic_name()).expect("Invalid topic");
        let () = agent
            .handle_restart_operation(&mqtt, &response_topic_restart)
            .await?;
        assert!(std::path::Path::new(&SLASH_RUN_PATH_TEDGE_AGENT_RESTART).exists());

        // removing the file
        let () = std::fs::remove_file(&SLASH_RUN_PATH_TEDGE_AGENT_RESTART).unwrap();

        Ok(())
    }
}
