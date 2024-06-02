use crate::availability::AvailabilityConfig;
use crate::availability::AvailabilityInput;
use crate::availability::AvailabilityOutput;
use crate::availability::TimerStart;
use async_trait::async_trait;
use c8y_api::smartrest::inventory::set_required_availability_message;
use c8y_api::smartrest::topic::C8yTopic;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_api::HealthStatus;
use tedge_api::Status;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::SetTimeout;
use tracing::debug;
use tracing::info;
use tracing::warn;

/// The timer payload. Keep it a struct in case if we need more data inside the payload in the future
/// `topic_id` is the EntityTopicId of the target device for availability monitoring
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TimerPayload {
    pub topic_id: EntityTopicId,
}

/// IDs can be retrieved from the registration message's payload
#[derive(Debug)]
struct DeviceIds {
    service_topic_id: ServiceTopicId,
    external_id: EntityExternalId,
}

#[derive(Debug)]
enum RegistrationResult {
    New,
    Update,
    Error(String),
}

pub struct AvailabilityActor {
    config: AvailabilityConfig,
    message_box: SimpleMessageBox<AvailabilityInput, AvailabilityOutput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    timer_sender: LoggingSender<TimerStart>,
    device_ids_map: HashMap<EntityTopicId, DeviceIds>,
    service_status_map: HashMap<ServiceTopicId, HealthStatus>,
}

#[async_trait]
impl Actor for AvailabilityActor {
    fn name(&self) -> &str {
        "AvailabilityActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        if !self.config.enable {
            info!("Device availability monitoring feature is disabled. To enable it, run 'tedge config set c8y.availability.enable true'");
            return Ok(());
        }

        self.init().await?;

        while let Some(input) = self.message_box.recv().await {
            match input {
                AvailabilityInput::MqttMessage(message) => {
                    self.process_mqtt_message(&message).await?;
                }
                AvailabilityInput::TimerComplete(event) => {
                    self.process_timer_complete(event.event).await?;
                }
            }
        }

        Ok(())
    }
}

impl AvailabilityActor {
    pub fn new(
        config: AvailabilityConfig,
        message_box: SimpleMessageBox<AvailabilityInput, AvailabilityOutput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        timer_sender: LoggingSender<TimerStart>,
    ) -> Self {
        Self {
            config,
            message_box,
            mqtt_publisher,
            timer_sender,
            device_ids_map: HashMap::new(),
            service_status_map: HashMap::new(),
        }
    }

    /// Init function to set up for the main device
    async fn init(&mut self) -> Result<(), RuntimeError> {
        let topic_id = EntityTopicId::default_main_device();

        self.device_ids_map.insert(
            topic_id.clone(),
            DeviceIds {
                service_topic_id: EntityTopicId::default_main_service("tedge-agent")
                    .unwrap()
                    .into(),
                external_id: self.config.main_device_id.clone(),
            },
        );

        self.send_smartrest_set_required_availability_for_main_device()
            .await?;

        self.start_heartbeat_timer_if_interval_is_positive(&topic_id)
            .await?;

        Ok(())
    }

    async fn process_mqtt_message(&mut self, message: &MqttMessage) -> Result<(), RuntimeError> {
        if let Ok((source, channel)) = self.config.mqtt_schema.entity_channel_of(&message.topic) {
            match channel {
                Channel::EntityMetadata => {
                    if let Ok(registration_message) = EntityRegistrationMessage::try_from(message) {
                        match registration_message.r#type {
                            EntityType::MainDevice => {
                                match self.update_device_service_pair(&registration_message) {
                                    RegistrationResult::New | RegistrationResult::Update => {
                                        self.start_heartbeat_timer_if_interval_is_positive(&source)
                                            .await?;
                                    }
                                    RegistrationResult::Error(reason) => {
                                        warn!(reason)
                                    }
                                }
                            }
                            EntityType::ChildDevice => {
                                match self.update_device_service_pair(&registration_message) {
                                    RegistrationResult::New => {
                                        self.send_smartrest_set_required_availability_for_child_device(&source)
                                            .await?;
                                        self.start_heartbeat_timer_if_interval_is_positive(&source)
                                            .await?;
                                    }
                                    RegistrationResult::Update => {
                                        self.start_heartbeat_timer_if_interval_is_positive(&source)
                                            .await?;
                                    }
                                    RegistrationResult::Error(reason) => warn!(reason),
                                }
                            }
                            EntityType::Service => {}
                        }
                    }
                }
                Channel::Health => {
                    if source.is_default_service() {
                        self.update_service_health_status(&source, message);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Insert a <"device topic ID" - "service topic ID" and "external ID"> pair into the map.
    /// If @health is provided in the registration message, use the value as long as it's valid as a service topic ID.
    /// If @health is not provided, use the "tedge-agent" service topic ID as default.
    /// @id is the only source to know the device's external ID. Hence, @id must be provided in the registration message.
    fn update_device_service_pair(
        &mut self,
        registration_message: &EntityRegistrationMessage,
    ) -> RegistrationResult {
        let source = &registration_message.topic_id;

        let result = match registration_message.other.get("@health") {
            None => {
                Ok(registration_message.topic_id.to_default_service_topic_id("tedge-agent").unwrap())
            }
            Some(raw_value) => {
                match raw_value.as_str() {
                    None => Err(format!("'@health' must hold a string value. Given: {raw_value:?}")),
                    Some(maybe_service_topic_id) => {
                        EntityTopicId::from_str(maybe_service_topic_id)
                            .map(|id| id.into())
                            .map_err(|_| format!("'@health' must be the default service topic schema 'device/DEVICE_NAME/service/SERVICE_NAME'. Given: {maybe_service_topic_id}"))
                    }
                }
            }
        };

        match result {
            Ok(service_topic_id) => {
                match registration_message.external_id.clone() {
                    None => RegistrationResult::Error(format!("'@id' field is missing. Cannot start availability monitoring for the device '{source}'")),
                    Some(external_id) => {
                        match self.device_ids_map
                            .insert(source.clone(), DeviceIds { service_topic_id, external_id }) {
                            None => RegistrationResult::New,
                            Some(_) => RegistrationResult::Update,
                        }
                    }
                }
            }
            Err(err) => RegistrationResult::Error(format!("'@health' contains invalid value in {source}. Details: {err}")),
        }
    }

    /// Set a new timer for heartbeat
    /// Caution: the heartbeat interval from config is defined in MINUTES, not seconds
    async fn start_heartbeat_timer_if_interval_is_positive(
        &mut self,
        source: &EntityTopicId,
    ) -> Result<(), RuntimeError> {
        if self.config.interval > 0 {
            let interval: u64 = self.config.interval.try_into().unwrap();
            self.timer_sender
                .send(SetTimeout::new(
                    Duration::from_secs(interval * 60),
                    TimerPayload {
                        topic_id: source.clone(),
                    },
                ))
                .await?;
        }

        Ok(())
    }

    /// Send SmartREST 117
    /// https://cumulocity.com/docs/smartrest/mqtt-static-templates/#117
    async fn send_smartrest_set_required_availability_for_main_device(
        &mut self,
    ) -> Result<(), RuntimeError> {
        let smartrest = set_required_availability_message(
            C8yTopic::SmartRestResponse,
            self.config.interval,
            &self.config.c8y_prefix,
        );
        self.mqtt_publisher.send(smartrest).await?;

        Ok(())
    }

    /// Send SmartREST 117
    /// https://cumulocity.com/docs/smartrest/mqtt-static-templates/#117
    async fn send_smartrest_set_required_availability_for_child_device(
        &mut self,
        source: &EntityTopicId,
    ) -> Result<(), RuntimeError> {
        if let Some(external_id) = self
            .device_ids_map
            .get(source)
            .map(|ids| ids.external_id.clone())
        {
            let smartrest = set_required_availability_message(
                C8yTopic::ChildSmartRestResponse(external_id.into()),
                self.config.interval,
                &self.config.c8y_prefix,
            );
            self.mqtt_publisher.send(smartrest).await?;
        }

        Ok(())
    }

    /// Insert a "service topic ID" - "health status" pair to the map.
    /// The received MQTT topic should be in default service schema: "device/+/service/+/status/health".
    fn update_service_health_status(&mut self, source: &EntityTopicId, message: &MqttMessage) {
        let health_status: HealthStatus =
            serde_json::from_slice(message.payload()).unwrap_or_default();
        self.service_status_map
            .insert(source.clone().into(), health_status);
    }

    async fn process_timer_complete(
        &mut self,
        timer_payload: TimerPayload,
    ) -> Result<(), RuntimeError> {
        let entity_topic_id = timer_payload.topic_id;
        if let Some((service_topic_id, external_id)) = self
            .device_ids_map
            .get(&entity_topic_id)
            .map(|ids| (&ids.service_topic_id, ids.external_id.as_ref()))
        {
            if let Some(health_status) = self.service_status_map.get(service_topic_id) {
                // Send an empty JSON over MQTT message if the target service status is "up"
                if health_status.status == Status::Up {
                    let json_over_mqtt_topic = format!(
                        "{prefix}/inventory/managedObjects/update/{external_id}",
                        prefix = self.config.c8y_prefix
                    );
                    let message =
                        MqttMessage::new(&Topic::new_unchecked(&json_over_mqtt_topic), "{}");
                    self.mqtt_publisher.send(message).await?;
                } else {
                    debug!("Heartbeat message is not sent because the status of the service '{service_topic_id}' is not 'up'");
                }
            }

            // Set a new timer
            self.start_heartbeat_timer_if_interval_is_positive(&entity_topic_id)
                .await?;
        };

        Ok(())
    }
}
