use crate::operation_workflows::actor::AgentInput;
use crate::operation_workflows::actor::InternalCommandState;
use crate::operation_workflows::actor::WorkflowActor;
use crate::operation_workflows::config::OperationConfig;
use crate::operation_workflows::message_box::CommandDispatcher;
use crate::state_repository::state::AgentStateRepository;
use log::error;
use std::process::Output;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::UnboundedLoggingReceiver;
use tedge_api::mqtt_topics::ChannelFilter::AnyCommand;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_script_ext::Execute;

pub struct WorkflowActorBuilder {
    config: OperationConfig,
    workflows: WorkflowSupervisor,
    input_sender: DynSender<AgentInput>,
    input_receiver: UnboundedLoggingReceiver<AgentInput>,
    command_dispatcher: CommandDispatcher,
    command_sender: DynSender<InternalCommandState>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    script_runner: ClientMessageBox<Execute, std::io::Result<Output>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl WorkflowActorBuilder {
    pub fn new(
        config: OperationConfig,
        workflows: WorkflowSupervisor,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        script_runner: &mut impl Service<Execute, std::io::Result<Output>>,
    ) -> Self {
        let (input_sender, input_receiver) = mpsc::unbounded();
        let (signal_sender, signal_receiver) = mpsc::channel(10);

        let input_receiver = UnboundedLoggingReceiver::new(
            "Mqtt-Request-Converter".into(),
            input_receiver,
            signal_receiver,
        );
        let input_sender: DynSender<AgentInput> = input_sender.into();

        let command_dispatcher = CommandDispatcher::default();
        let command_sender = input_sender.sender_clone();

        let mqtt_publisher = mqtt_actor.get_sender();
        mqtt_actor.connect_sink(
            Self::subscriptions(&config.mqtt_schema, &config.device_topic_id),
            &input_sender,
        );
        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_publisher);

        let script_runner = ClientMessageBox::new(script_runner);

        Self {
            config,
            workflows,
            input_sender,
            input_receiver,
            command_dispatcher,
            command_sender,
            mqtt_publisher,
            signal_sender,
            script_runner,
        }
    }

    /// Register an actor to handle a builtin operation
    pub fn register_builtin_operation<OperationActor>(&mut self, actor: &mut OperationActor)
    where
        OperationActor: MessageSource<GenericCommandData, NoConfig>,
        for<'a> &'a OperationActor:
            IntoIterator<Item = (OperationName, DynSender<GenericCommandState>)>,
    {
        actor.connect_sink(NoConfig, &self.input_sender);
        for (operation, sender) in actor.into_iter() {
            self.command_dispatcher
                .register_operation_handler(operation, sender)
        }
    }

    pub fn subscriptions(mqtt_schema: &MqttSchema, device_topic_id: &EntityTopicId) -> TopicFilter {
        mqtt_schema.topics(EntityFilter::Entity(device_topic_id), AnyCommand)
    }
}

impl RuntimeRequestSink for WorkflowActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<WorkflowActor> for WorkflowActorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<WorkflowActor, Self::Error> {
        Ok(self.build())
    }

    fn build(mut self) -> WorkflowActor {
        for capability in self.command_dispatcher.capabilities() {
            if let Err(err) = self
                .workflows
                .register_builtin_workflow(capability.as_str().into())
            {
                error!("Fail to register built-in workflow for {capability} operation: {err}");
            }
        }

        let repository =
            AgentStateRepository::new(self.config.state_dir, self.config.config_dir, "workflows");
        WorkflowActor {
            mqtt_schema: self.config.mqtt_schema,
            device_topic_id: self.config.device_topic_id,
            workflows: self.workflows,
            state_repository: repository,
            log_dir: self.config.log_dir,
            input_receiver: self.input_receiver,
            builtin_command_dispatcher: self.command_dispatcher,
            mqtt_publisher: self.mqtt_publisher,
            command_sender: self.command_sender,
            script_runner: self.script_runner,
        }
    }
}
