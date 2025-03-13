mod actor;
mod config;
mod error;
mod message;
mod operation;
mod worker;

#[cfg(test)]
mod tests;

use actor::FirmwareInput;
use actor::FirmwareManagerActor;
pub use config::*;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_api::path::DataDir;
use tedge_config::models::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::FileError;
use worker::IdDownloadRequest;
use worker::IdDownloadResult;
use worker::OperationOutcome;

pub struct FirmwareManagerBuilder {
    config: FirmwareManagerConfig,
    input_receiver: LoggingReceiver<FirmwareInput>,
    mqtt_publisher: DynSender<MqttMessage>,
    download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    progress_sender: DynSender<OperationOutcome>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl FirmwareManagerBuilder {
    pub fn try_new(
        config: FirmwareManagerConfig,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        downloader_actor: &mut impl Service<IdDownloadRequest, IdDownloadResult>,
    ) -> Result<FirmwareManagerBuilder, FileError> {
        Self::init(&config.data_dir)?;

        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver = LoggingReceiver::new(
            "C8Y-Firmware-Manager".into(),
            input_receiver,
            signal_receiver,
        );
        let mqtt_sender: DynSender<MqttMessage> = input_sender.sender_clone();

        mqtt_actor.connect_sink(Self::subscriptions(&config.c8y_prefix), &mqtt_sender);
        let mqtt_publisher = mqtt_actor.get_sender();
        let download_sender = ClientMessageBox::new(downloader_actor);
        let progress_sender = input_sender.into();
        Ok(Self {
            config,
            input_receiver,
            mqtt_publisher,
            download_sender,
            progress_sender,
            signal_sender,
        })
    }

    pub fn init(data_dir: &DataDir) -> Result<(), FileError> {
        create_directory_with_defaults(data_dir.cache_dir())?;
        create_directory_with_defaults(data_dir.file_transfer_dir())?;
        create_directory_with_defaults(data_dir.firmware_dir())?;
        Ok(())
    }

    fn subscriptions(prefix: &TopicPrefix) -> TopicFilter {
        vec![
            &format!("{prefix}/s/ds"),
            "tedge/+/commands/res/firmware_update",
        ]
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
        Ok(FirmwareManagerActor::new(
            self.config,
            self.input_receiver,
            self.mqtt_publisher,
            self.download_sender,
            self.progress_sender,
        ))
    }
}
