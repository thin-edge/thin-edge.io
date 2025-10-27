use crate::operation_workflows::actor::AgentInput;
use crate::operation_workflows::actor::InternalCommandState;
use crate::operation_workflows::actor::WorkflowActor;
use crate::operation_workflows::config::OperationConfig;
use crate::operation_workflows::message_box::CommandDispatcher;
use crate::operation_workflows::message_box::SyncSignalDispatcher;
use crate::operation_workflows::persist::WorkflowRepository;
use crate::state_repository::state::agent_state_dir;
use crate::state_repository::state::AgentStateRepository;
use std::path::PathBuf;
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
use tedge_api::commands::CmdMetaSyncSignal;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::SyncOnCommand;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_script_ext::Execute;

pub struct WorkflowActorBuilder {
    config: OperationConfig,
    input_sender: DynSender<AgentInput>,
    input_receiver: UnboundedLoggingReceiver<AgentInput>,
    command_dispatcher: CommandDispatcher,
    sync_signal_dispatcher: SyncSignalDispatcher,
    command_sender: DynSender<InternalCommandState>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    script_runner: ClientMessageBox<Execute, std::io::Result<Output>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl WorkflowActorBuilder {
    pub fn new(
        config: OperationConfig,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        script_runner: &mut impl Service<Execute, std::io::Result<Output>>,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
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

        let sync_signal_dispatcher = SyncSignalDispatcher::default();

        let mqtt_publisher = mqtt_actor.get_sender();
        mqtt_actor.connect_sink(
            Self::subscriptions(
                &config.mqtt_schema,
                &config.device_topic_id,
                &config.service_topic_id,
            ),
            &input_sender,
        );
        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_publisher);

        let script_runner = ClientMessageBox::new(script_runner);

        fs_notify.connect_sink(config.operations_dir.clone().into(), &input_sender);

        Self {
            config,
            input_sender,
            input_receiver,
            command_dispatcher,
            sync_signal_dispatcher,
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
                .register_operation_handler(operation, sender);
        }
    }

    /// Register an actor to receive sync signals on completion of other commands
    pub fn register_sync_signal_sink<OperationActor>(
        &mut self,
        op_type: OperationType,
        actor: &OperationActor,
    ) where
        OperationActor: MessageSink<CmdMetaSyncSignal> + SyncOnCommand,
    {
        let sender = actor.get_sender();
        self.sync_signal_dispatcher
            .register_operation_handler(op_type, sender.sender_clone());
        for operation in actor.sync_on_commands() {
            self.sync_signal_dispatcher
                .register_operation_listener(operation, sender.sender_clone());
        }
    }

    pub fn subscriptions(
        mqtt_schema: &MqttSchema,
        device_topic_id: &EntityTopicId,
        service_topic_id: &EntityTopicId,
    ) -> TopicFilter {
        let mut topics = mqtt_schema.topics(
            EntityFilter::Entity(device_topic_id),
            ChannelFilter::AnyCommand,
        );
        topics.add_all(mqtt_schema.topics(
            EntityFilter::Entity(service_topic_id),
            ChannelFilter::AnySignal,
        ));
        topics
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

    fn build(self) -> WorkflowActor {
        let builtin_workflows = self.command_dispatcher.capabilities();
        let custom_workflows_dir = self.config.operations_dir;
        let state_dir = agent_state_dir(self.config.state_dir, self.config.config_dir);
        let workflow_repository =
            WorkflowRepository::new(builtin_workflows, custom_workflows_dir, state_dir.clone());
        let state_repository = AgentStateRepository::with_state_dir(state_dir, "workflows");

        WorkflowActor {
            mqtt_schema: self.config.mqtt_schema,
            device_topic_id: self.config.device_topic_id,
            workflow_repository,
            state_repository,
            log_dir: self.config.log_dir,
            input_receiver: self.input_receiver,
            builtin_command_dispatcher: self.command_dispatcher,
            sync_signal_dispatcher: self.sync_signal_dispatcher,
            mqtt_publisher: self.mqtt_publisher,
            command_sender: self.command_sender,
            script_runner: self.script_runner,
        }
    }
}
