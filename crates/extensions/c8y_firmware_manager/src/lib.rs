mod actor;
mod config;
mod error;
mod message;
mod operation;
#[cfg(test)]
mod tests;

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
pub use config::*;
use std::path::Path;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_config::DEFAULT_FILE_TRANSFER_DIR_NAME;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_timer_ext::SetTimeout;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;

pub struct FirmwareManagerBuilder {
    config: FirmwareManagerConfig,
    input_receiver: LoggingReceiver<FirmwareInput>,
    mqtt_publisher: DynSender<MqttMessage>,
    jwt_retriever: JwtRetriever,
    timer_sender: DynSender<SetTimeout<OperationKey>>,
    download_sender: DynSender<IdDownloadRequest>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl FirmwareManagerBuilder {
    pub fn try_new(
        config: FirmwareManagerConfig,
        mqtt_actor: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        jwt_actor: &mut impl ServiceProvider<JwtRequest, JwtResult, NoConfig>,
        timer_actor: &mut impl ServiceProvider<OperationSetTimeout, OperationTimeout, NoConfig>,
        downloader_actor: &mut impl ServiceProvider<IdDownloadRequest, IdDownloadResult, NoConfig>,
    ) -> Result<FirmwareManagerBuilder, FileError> {
        Self::init(&config.data_dir)?;

        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver = LoggingReceiver::new(
            "C8Y-Firmware-Manager".into(),
            input_receiver,
            signal_receiver,
        );

        let mqtt_publisher =
            mqtt_actor.connect_consumer(Self::subscriptions(), input_sender.clone().into());
        let jwt_retriever = JwtRetriever::new("Firmware => JWT", jwt_actor);
        let timer_sender = timer_actor.connect_consumer(NoConfig, input_sender.clone().into());
        let download_sender = downloader_actor.connect_consumer(NoConfig, input_sender.into());
        Ok(Self {
            config,
            input_receiver,
            mqtt_publisher,
            jwt_retriever,
            timer_sender,
            download_sender,
            signal_sender,
        })
    }

    pub fn init(data_dir: &Path) -> Result<(), FileError> {
        create_directory_with_defaults(data_dir.join("cache"))?;
        create_directory_with_defaults(data_dir.join(DEFAULT_FILE_TRANSFER_DIR_NAME))?;
        create_directory_with_defaults(data_dir.join("firmware"))?;
        Ok(())
    }

    pub fn subscriptions() -> TopicFilter {
        vec!["c8y/s/ds", "tedge/+/commands/res/firmware_update"]
            .try_into()
            .expect("Infallible")
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
        let peers = FirmwareManagerMessageBox::new(
            self.input_receiver,
            self.mqtt_publisher,
            self.jwt_retriever,
            self.timer_sender,
            self.download_sender,
        );

        Ok(FirmwareManagerActor::new(self.config, peers))
    }
}
