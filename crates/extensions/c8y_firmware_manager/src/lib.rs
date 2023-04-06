mod actor;
mod config;
mod error;
mod message;
mod operation;

#[cfg(test)]
mod tests;

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::operation::OperationKey;
use actor::FirmwareInput;
use actor::FirmwareManagerActor;
use actor::FirmwareManagerMessageBox;
use c8y_http_proxy::credentials::JwtResult;
use c8y_http_proxy::credentials::JwtRetriever;
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
    jwt_retriever: Option<JwtRetriever>,
    timer_sender: Option<DynSender<SetTimeout<OperationKey>>>,
    download_sender: Option<DynSender<IdDownloadRequest>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl FirmwareManagerBuilder {
    pub fn new(config: FirmwareManagerConfig) -> FirmwareManagerBuilder {
        let (events_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let receiver = LoggingReceiver::new(
            "C8Y-Firmware-Manager".into(),
            input_receiver,
            signal_receiver,
        );

        Self {
            config,
            receiver,
            events_sender,
            mqtt_publisher: None,
            jwt_retriever: None,
            timer_sender: None,
            download_sender: None,
            signal_sender,
        }
    }

    /// Connect this config manager instance to jwt token actor
    pub fn with_jwt_token(
        &mut self,
        jwt: &mut impl ServiceProvider<(), JwtResult, NoConfig>,
    ) -> Result<(), LinkError> {
        self.jwt_retriever = Some(JwtRetriever::new("Firmware => JWT", jwt));
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

impl ServiceConsumer<IdDownloadRequest, IdDownloadResult, NoConfig> for FirmwareManagerBuilder {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<IdDownloadRequest>) {
        self.download_sender = Some(sender);
    }

    fn get_response_sender(&self) -> DynSender<IdDownloadResult> {
        self.events_sender.clone().into()
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for FirmwareManagerBuilder {
    fn get_config(&self) -> TopicFilter {
        vec!["c8y/s/ds", "tedge/+/commands/res/firmware_update"]
            .try_into()
            .expect("Infallible")
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

impl Builder<FirmwareManagerActor> for FirmwareManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<FirmwareManagerActor, Self::Error> {
        let mqtt_publisher = self.mqtt_publisher.ok_or_else(|| LinkError::MissingPeer {
            role: "mqtt".to_string(),
        })?;

        let jwt_retriever = self.jwt_retriever.ok_or_else(|| LinkError::MissingPeer {
            role: "jwt".to_string(),
        })?;

        let timer_sender = self.timer_sender.ok_or_else(|| LinkError::MissingPeer {
            role: "timer".to_string(),
        })?;

        let download_requester = self.download_sender.ok_or_else(|| LinkError::MissingPeer {
            role: "downloader".to_string(),
        })?;

        let peers = FirmwareManagerMessageBox::new(
            self.receiver,
            mqtt_publisher,
            jwt_retriever,
            timer_sender,
            download_requester,
        );

        Ok(FirmwareManagerActor::new(self.config, peers))
    }
}
