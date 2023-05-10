use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::tedge_operation_converter::actor::AgentInput;
use crate::tedge_operation_converter::actor::TedgeOperationConverterActor;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct TedgeOperationConverterBuilder {
    input_receiver: LoggingReceiver<AgentInput>,
    software_sender: LoggingSender<SoftwareRequest>,
    restart_sender: LoggingSender<RestartOperationRequest>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl TedgeOperationConverterBuilder {
    pub fn new(
        software_actor: &mut impl ServiceProvider<SoftwareRequest, SoftwareResponse, NoConfig>,
        restart_actor: &mut impl ServiceProvider<
            RestartOperationRequest,
            RestartOperationResponse,
            NoConfig,
        >,
        mqtt_actor: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
    ) -> Self {
        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);

        let input_receiver = LoggingReceiver::new(
            "Mqtt-Request-Converter".into(),
            input_receiver,
            signal_receiver,
        );

        let software_sender =
            software_actor.connect_consumer(NoConfig, input_sender.clone().into());
        let software_sender = LoggingSender::new("SoftwareSender".into(), software_sender);

        let restart_sender = restart_actor.connect_consumer(NoConfig, input_sender.clone().into());
        let restart_sender = LoggingSender::new("RestartSender".into(), restart_sender);

        let mqtt_publisher =
            mqtt_actor.connect_consumer(Self::subscriptions(), input_sender.into());
        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_publisher);

        Self {
            input_receiver,
            software_sender,
            restart_sender,
            mqtt_publisher,
            signal_sender,
        }
    }

    pub fn subscriptions() -> TopicFilter {
        vec![
            "tedge/commands/req/software/list",
            "tedge/commands/req/software/update",
            "tedge/commands/req/control/restart",
        ]
        .try_into()
        .expect("Infallible")
    }
}

impl RuntimeRequestSink for TedgeOperationConverterBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<TedgeOperationConverterActor> for TedgeOperationConverterBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<TedgeOperationConverterActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> TedgeOperationConverterActor {
        TedgeOperationConverterActor::new(
            self.input_receiver,
            self.software_sender,
            self.restart_sender,
            self.mqtt_publisher,
        )
    }
}
