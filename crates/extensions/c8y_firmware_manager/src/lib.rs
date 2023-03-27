mod actor;
mod config;
mod error;
mod message;
mod operation;

#[cfg(test)]
mod tests;

use crate::operation::OperationKey;
use actor::FirmwareInput;
use actor::FirmwareManagerActor;
use actor::FirmwareManagerMessageBox;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
pub use config::*;
use mqtt_channel::TopicFilter;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_mqtt_ext::MqttMessage;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

pub struct FirmwareManagerBuilder {
    config: FirmwareManagerConfig,
    receiver: LoggingReceiver<FirmwareInput>,
    events_sender: mpsc::Sender<FirmwareInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    c8y_http_proxy: Option<C8YHttpProxy>,
    timer_sender: Option<DynSender<SetTimeout<OperationKey>>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl FirmwareManagerBuilder {
    pub fn new(config: FirmwareManagerConfig) -> FirmwareManagerBuilder {
        let (events_sender, events_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let receiver = LoggingReceiver::new(
            "C8Y-Firmware-Manager".into(),
            events_receiver,
            signal_receiver,
        );

        Self {
            config,
            receiver,
            events_sender,
            mqtt_publisher: None,
            c8y_http_proxy: None,
            timer_sender: None,
            signal_sender,
        }
    }

    /// Connect this config manager instance to some http connection provider
    pub fn with_c8y_http_proxy(
        &mut self,
        http: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
    ) -> Result<(), LinkError> {
        self.c8y_http_proxy = Some(C8YHttpProxy::new("FirmwareManager => C8Y", http));
        Ok(())
    }
}

impl ServiceConsumer<SetTimeout<OperationKey>, Timeout<OperationKey>, NoConfig>
    for FirmwareManagerBuilder
{
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<SetTimeout<OperationKey>>) {
        self.timer_sender = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<Timeout<OperationKey>> {
        self.events_sender.clone().into()
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for FirmwareManagerBuilder {
    fn get_config(&self) -> TopicFilter {
        vec!["c8y/s/ds", "tedge/+/commands/res/firmware_update"]
            .try_into()
            .unwrap()
    }

    fn set_request_sender(&mut self, request_sender: DynSender<MqttMessage>) {
        self.mqtt_publisher = Some(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        self.events_sender.clone().into()
    }
}

impl RuntimeRequestSink for FirmwareManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<(FirmwareManagerActor, FirmwareManagerMessageBox)> for FirmwareManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<(FirmwareManagerActor, FirmwareManagerMessageBox), Self::Error> {
        let mqtt_publisher = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let c8y_http_proxy = self.c8y_http_proxy.ok_or_else(|| LinkError::MissingPeer {
            role: "c8y-http".to_string(),
        })?;

        let timer_sender = self.timer_sender.ok_or_else(|| LinkError::MissingPeer {
            role: "timer".to_string(),
        })?;

        let peers = FirmwareManagerMessageBox::new(
            self.receiver,
            mqtt_publisher,
            c8y_http_proxy,
            timer_sender,
        );

        let actor = FirmwareManagerActor::new(self.config);

        Ok((actor, peers))
    }
}
