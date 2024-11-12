use async_trait::async_trait;
use log::error;
use log::info;
use std::sync::Arc;
use std::sync::Mutex;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::EntityStore;
use tedge_mqtt_ext::MqttMessage;

pub struct EntityManagerActor {
    input_receiver: LoggingReceiver<MqttMessage>,
    entity_store: Arc<Mutex<EntityStore>>,
}

impl EntityManagerActor {
    pub(crate) fn new(
        input_receiver: LoggingReceiver<MqttMessage>,
        entity_store: Arc<Mutex<EntityStore>>,
    ) -> Self {
        Self {
            input_receiver,
            entity_store,
        }
    }
}

#[async_trait]
impl Actor for EntityManagerActor {
    fn name(&self) -> &str {
        "EntityManagerActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(input) = self.input_receiver.recv().await {
            match EntityRegistrationMessage::try_from(&input) {
                Ok(register_message) => {
                    info!("Hippo: Received reg message: {:?}", register_message);
                    if let Err(e) = self.entity_store.lock().unwrap().update(register_message) {
                        error!("Entity registration failed: {e}");
                    }
                }
                Err(_) => {
                    error!(
                        "Invalid entity registration message received on: {}",
                        &input.topic.name
                    );
                }
            }
        }

        Ok(())
    }
}
