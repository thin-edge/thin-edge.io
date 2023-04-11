mod actor;
mod config;
mod error;
mod message;
mod operation;
#[cfg(test)]
mod tests;

pub use config::*;

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::OperationSetTimeout;
use crate::actor::OperationTimeout;
use crate::operation::OperationKey;

use actor::FirmwareInput;
use actor::FirmwareManagerActor;
use actor::FirmwareManagerMessageBox;
use c8y_http_proxy::credentials::JwtRequest;
use c8y_http_proxy::credentials::JwtResult;
use c8y_http_proxy::credentials::JwtRetriever;
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
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;

pub struct FirmwareManagerBuilder {
    config: FirmwareManagerConfig,
    input_receiver: LoggingReceiver<FirmwareInput>,
    input_sender: mpsc::Sender<FirmwareInput>,
    mqtt_publisher: Option<DynSender<MqttMessage>>,
    jwt_retriever: Option<JwtRetriever>,
    timer_sender: Option<DynSender<SetTimeout<OperationKey>>>,
    download_sender: Option<DynSender<IdDownloadRequest>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl FirmwareManagerBuilder {
    pub fn new(
        config: FirmwareManagerConfig,
        mqtt_actor: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        jwt_actor: &mut impl ServiceProvider<JwtRequest, JwtResult, NoConfig>,
        timer_actor: &mut impl ServiceProvider<OperationSetTimeout, OperationTimeout, NoConfig>,
        downloader_actor: &mut impl ServiceProvider<IdDownloadRequest, IdDownloadResult, NoConfig>,
    ) -> FirmwareManagerBuilder {
        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver = LoggingReceiver::new(
            "C8Y-Firmware-Manager".into(),
            input_receiver,
            signal_receiver,
        );

        let mut firmware_manager_builder = Self {
            config,
            input_receiver,
            input_sender,
            mqtt_publisher: None,
            jwt_retriever: None,
            timer_sender: None,
            download_sender: None,
            signal_sender,
        };

        firmware_manager_builder.with_jwt_token_retriever(jwt_actor);
        firmware_manager_builder.set_connection(mqtt_actor);
        firmware_manager_builder.set_connection(timer_actor);
        firmware_manager_builder.set_connection(downloader_actor);

        firmware_manager_builder
    }

    /// Connect this config manager instance to jwt token actor
    pub fn with_jwt_token_retriever(
        &mut self,
        jwt: &mut impl ServiceProvider<(), JwtResult, NoConfig>,
    ) {
        self.jwt_retriever = Some(JwtRetriever::new("Firmware => JWT", jwt));
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
        self.input_sender.clone().into()
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
        self.input_sender.clone().into()
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
        self.input_sender.clone().into()
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
            self.input_receiver,
            mqtt_publisher,
            jwt_retriever,
            timer_sender,
            download_requester,
        );

        Ok(FirmwareManagerActor::new(self.config, peers))
    }
}
