use async_trait::async_trait;
use log::info;
use std::collections::HashSet;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeAction;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeEvent;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_mqtt_ext::MqttMessage;

use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;

fan_in_message_type!(HealthInput[MqttMessage, RuntimeEvent] : Debug);

pub struct HealthMonitorActor {
    daemon_name: String,
    messages: SimpleMessageBox<HealthInput, MqttMessage>,
    runtime: DynSender<RuntimeAction>,
    watched: HashSet<String>,
    expected: HashSet<String>,
}

impl HealthMonitorActor {
    pub fn new(
        daemon_name: String,
        messages: SimpleMessageBox<HealthInput, MqttMessage>,
        runtime: DynSender<RuntimeAction>,
    ) -> Self {
        let watched = HashSet::new();
        let expected = HashSet::new();
        Self {
            daemon_name,
            messages,
            runtime,
            watched,
            expected,
        }
    }

    pub fn up_health_status(&self) -> MqttMessage {
        health_status_up_message(&self.daemon_name)
    }

    pub fn down_health_status(&self) -> MqttMessage {
        health_status_down_message(&self.daemon_name)
    }

    async fn check_actor_as_running(&mut self, actor: String) -> Result<(), ChannelError> {
        self.expected.remove(&actor);
        if self.expected.is_empty() {
            self.messages.send(self.up_health_status()).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Actor for HealthMonitorActor {
    fn name(&self) -> &str {
        "HealthMonitorActor"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.messages.send(self.up_health_status()).await?;

        while let Ok(Some(message)) = self.messages.try_recv().await {
            match message {
                HealthInput::MqttMessage(_) => {
                    // FIXME consider to set a timer after which non-responding actors are considered as blocked
                    self.expected = self.watched.clone();
                    self.runtime.send(RuntimeAction::status_request()).await?;
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Running { task, .. }) => {
                    self.check_actor_as_running(task).await?;
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Started { task }) => {
                    // FIXME the list of watched actor should not be dynamic
                    // FIXME FsWatcher is not sending status
                    // FIXME Signal-Handler is not sending status
                    // FIXME C8Y-REST is not sending status
                    if !task.contains("FsWatcher")
                        && !task.contains("Signal-Handler")
                        && !task.contains("C8Y-REST")
                        && !task.contains("HttpFileTransferServer")
                        && !task.contains("HealthMonitorActor")
                    {
                        self.watched.insert(task);
                    }
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Stopped { task }) => {
                    // FIXME One should be able to control this behavior depending on the actor
                    info!("Keep running without {task}");
                    self.watched.remove(&task);
                    self.check_actor_as_running(task).await?;
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Aborted { task, error }) => {
                    // FIXME One should be able to control this behavior depending on the actor
                    info!("Aborting on {task} error: {error}");
                    self.runtime.send(RuntimeAction::shutdown_request()).await?;
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Error(error)) => {
                    info!("Aborting on runtime error: {}", &error);
                    return Err(error);
                }
            }
        }

        // A shutdown has been requested.
        // Keep monitoring the actors till they all stopped.
        while let Some(message) = self.messages.recv().await {
            match message {
                HealthInput::RuntimeEvent(RuntimeEvent::Stopped { task })
                | HealthInput::RuntimeEvent(RuntimeEvent::Aborted { task, .. }) => {
                    self.watched.remove(&task);
                    if self.watched.is_empty() {
                        break;
                    }
                }
                HealthInput::RuntimeEvent(RuntimeEvent::Error(_)) => {
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
