//! Supervise the actors of an application
//!
use crate::run_actor::RunActor;
use crate::Actor;
use crate::Builder;
use crate::ChannelError;
use crate::DynSender;
use crate::MessageSink;
use crate::RuntimeError;
use crate::RuntimeRequestSink;
use futures::channel::mpsc;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use std::collections::HashMap;
use std::panic;
use std::time::Duration;
use tokio::task::JoinError;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::instrument;

// TODO: set back to 60
const ACTORS_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Actions sent by actors to the runtime
#[derive(Debug)]
pub enum RuntimeAction {
    Shutdown,
    Spawn(RunActor),
}

/// Requests sent by the runtime to actors
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeRequest {
    Shutdown,
}

/// Events published by the runtime
#[derive(Debug)]
pub enum RuntimeEvent {
    Error(RuntimeError),
    Started { task: String },
    Stopped { task: String },
    Aborted { task: String, error: String },
}

/// The actor runtime
pub struct Runtime {
    handle: RuntimeHandle,
    bg_task: JoinHandle<Result<(), RuntimeError>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

impl Runtime {
    /// Launch the runtime, returning a runtime handler
    ///
    /// TODO: ensure this can only be called once
    pub fn new() -> Runtime {
        Self::with_events_sender(None)
    }

    fn with_events_sender(events_sender: Option<DynSender<RuntimeEvent>>) -> Runtime {
        let (actions_sender, actions_receiver) = mpsc::channel(16);
        let runtime_actor = RuntimeActor::new(actions_receiver, events_sender, ACTORS_EXIT_TIMEOUT);

        let runtime_task = tokio::spawn(runtime_actor.run());
        Runtime {
            handle: RuntimeHandle { actions_sender },
            bg_task: runtime_task,
        }
    }

    pub fn get_handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }

    /// Spawn an actor
    pub async fn spawn<T, A>(&mut self, actor_builder: T) -> Result<(), RuntimeError>
    where
        T: Builder<A> + RuntimeRequestSink,
        A: Actor,
    {
        self.handle.spawn(actor_builder).await
    }

    /// Run the runtime up to completion
    ///
    /// I.e until
    /// - Either, a `Shutdown` action is sent to the runtime
    /// - Or, all the runtime handler clones have been dropped
    ///       and all the running tasks have reach completion (successfully or not).
    pub async fn run_to_completion(self) -> Result<(), RuntimeError> {
        if let Err(err) = Runtime::wait_for_completion(self.bg_task).await {
            error!("Aborted due to {err}");
            std::process::exit(1)
        }

        Ok(())
    }

    async fn wait_for_completion(
        bg_task: JoinHandle<Result<(), RuntimeError>>,
    ) -> Result<(), RuntimeError> {
        match bg_task.await {
            Ok(result) => result,
            Err(err) if err.is_panic() => Err(RuntimeError::RuntimePanic),
            Err(_) => Err(RuntimeError::RuntimeCancellation),
        }
    }
}

/// A handle passed to actors to interact with the runtime
#[derive(Clone)]
pub struct RuntimeHandle {
    actions_sender: mpsc::Sender<RuntimeAction>,
}

impl RuntimeHandle {
    /// Stop all the actors and the runtime
    pub async fn shutdown(&mut self) -> Result<(), RuntimeError> {
        Ok(self.send(RuntimeAction::Shutdown).await?)
    }

    /// Spawn an actor
    pub async fn spawn<A, T>(&mut self, actor_builder: T) -> Result<(), RuntimeError>
    where
        A: Actor,
        T: Builder<A> + RuntimeRequestSink,
    {
        let run_actor = RunActor::from_builder(actor_builder);

        Ok(self.send(RuntimeAction::Spawn(run_actor)).await?)
    }

    /// Send an action to the runtime
    async fn send(&mut self, action: RuntimeAction) -> Result<(), ChannelError> {
        debug!(target: "Runtime", "schedule {:?}", action);
        self.actions_sender.send(action).await?;
        Ok(())
    }
}

impl MessageSink<RuntimeAction> for RuntimeHandle {
    fn get_sender(&self) -> DynSender<RuntimeAction> {
        self.actions_sender.clone().into()
    }
}

/// The actual runtime implementation
struct RuntimeActor {
    actions: mpsc::Receiver<RuntimeAction>,
    events: Option<DynSender<RuntimeEvent>>,
    cleanup_duration: Duration,
    futures: FuturesUnordered<JoinHandle<Result<String, (String, RuntimeError)>>>,
    running_actors: HashMap<String, DynSender<RuntimeRequest>>,
}

impl RuntimeActor {
    fn new(
        actions: mpsc::Receiver<RuntimeAction>,
        events: Option<DynSender<RuntimeEvent>>,
        cleanup_duration: Duration,
    ) -> Self {
        Self {
            actions,
            events,
            cleanup_duration,
            futures: FuturesUnordered::new(),
            running_actors: HashMap::default(),
        }
    }

    #[instrument(name = "Runtime", level = "trace", skip_all)]
    async fn run(mut self) -> Result<(), RuntimeError> {
        info!(target: "Runtime", "Started");
        let mut aborting_error = None;
        let mut actors_count: usize = 0;
        loop {
            tokio::select! {
                action = self.actions.next() => {
                    match action {
                        Some(action) => {
                            match action {
                                RuntimeAction::Spawn(actor) => {
                                    let running_name = format!("{}-{}", actor.name(), actors_count);
                                    info!(target: "Runtime", "Running {running_name}");
                                    self.send_event(RuntimeEvent::Started {
                                        task: running_name.clone(),
                                    })
                                    .await;
                                    self.running_actors.insert(running_name.clone(), actor.get_signal_sender());
                                    self.futures.push(tokio::spawn(run_task(actor, running_name)));
                                    actors_count += 1;
                               }
                               RuntimeAction::Shutdown => {
                                    info!(target: "Runtime", "Shutting down");
                                    shutdown_actors(&mut self.running_actors).await;
                                    break;
                               }
                            }
                        }
                        None => {
                            info!(target: "Runtime", "Runtime actions channel closed, runtime stopping");
                            shutdown_actors(&mut self.running_actors).await;
                            break;
                        }
                    }
                },
                Some(finished_actor) = self.futures.next() => {
                    if let Err(error) = self.handle_actor_finishing(finished_actor).await {
                        info!(target: "Runtime", "Shutting down on error: {error}");
                        aborting_error = Some(error);
                        shutdown_actors(&mut self.running_actors).await;
                        break
                    }
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(self.cleanup_duration) => {
                error!(target: "Runtime", "Timeout waiting for all actors to shutdown");
                for still_running in self.running_actors.keys() {
                     error!(target: "Runtime", "Failed to shutdown: {still_running}")
                }
            }
            _ = self.wait_for_actors_to_finish() => info!(target: "Runtime", "All actors have finished")
        }

        match aborting_error {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    async fn wait_for_actors_to_finish(&mut self) {
        while let Some(finished_actor) = self.futures.next().await {
            let _ = self.handle_actor_finishing(finished_actor).await;
        }
    }

    async fn handle_actor_finishing(
        &mut self,
        finished_actor: Result<Result<String, (String, RuntimeError)>, JoinError>,
    ) -> Result<(), RuntimeError> {
        match finished_actor {
            Err(e) => {
                error!(target: "Runtime", "Failed to execute actor: {e}");
                Err(RuntimeError::JoinError(e))
            }
            Ok(Ok(actor)) => {
                self.running_actors.remove(&actor);
                info!(target: "Runtime", "Actor has finished: {actor}");
                self.send_event(RuntimeEvent::Stopped { task: actor }).await;
                Ok(())
            }
            Ok(Err((actor, error))) => {
                self.running_actors.remove(&actor);
                error!(target: "Runtime", "Actor {actor} has finished unsuccessfully: {error:?}");
                self.send_event(RuntimeEvent::Aborted {
                    task: actor.clone(),
                    error: format!("{error}"),
                })
                .await;
                Err(error)
            }
        }
    }

    async fn send_event(&mut self, event: RuntimeEvent) {
        if let Some(events) = &mut self.events {
            if let Err(e) = events.send(event).await {
                error!(target: "Runtime", "Failed to send RuntimeEvent: {e}");
            }
        }
    }
}

async fn shutdown_actors<'a, I>(a: I)
where
    I: IntoIterator<Item = (&'a String, &'a mut DynSender<RuntimeRequest>)>,
{
    for (running_as, sender) in a {
        match sender.send(RuntimeRequest::Shutdown).await {
            Ok(()) => {
                debug!(target: "Runtime", "Successfully sent shutdown request to {running_as}")
            }
            Err(e) => {
                error!(target: "Runtime", "Failed to send shutdown request to {running_as}: {e:?}")
            }
        }
    }
}

async fn run_task(task: RunActor, running_name: String) -> Result<String, (String, RuntimeError)> {
    match tokio::spawn(task.run()).await {
        Ok(r) => r
            .map(|_| running_name.clone())
            .map_err(|e| (running_name, e)),
        Err(e) => Err((running_name.clone(), e.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_in_message_type;
    use crate::message_boxes::MessageReceiver;
    use crate::LoggingReceiver;
    use crate::LoggingSender;
    use crate::Message;
    use crate::SimpleMessageBox;
    use async_trait::async_trait;
    use futures::channel::mpsc;
    use std::time::Duration;

    fan_in_message_type!(EchoMessage[String, RuntimeRequest] : Debug, PartialEq);

    pub struct Echo {
        messages: SimpleMessageBox<EchoMessage, EchoMessage>,
    }

    impl Echo {
        fn new(messages: SimpleMessageBox<EchoMessage, EchoMessage>) -> Self {
            Self { messages }
        }
    }

    #[async_trait]
    impl Actor for Echo {
        fn name(&self) -> &str {
            "Echo"
        }

        async fn run(mut self) -> Result<(), RuntimeError> {
            while let Some(message) = self.messages.recv().await {
                match message {
                    EchoMessage::String(message) => {
                        crate::Sender::send(&mut self.messages, EchoMessage::String(message))
                            .await?
                    }
                    EchoMessage::RuntimeRequest(RuntimeRequest::Shutdown) => {
                        dbg!("shutdown requested");
                        crate::Sender::send(
                            &mut self.messages,
                            EchoMessage::String("Echo stopped".to_string()),
                        )
                        .await?;
                        break;
                    }
                }
            }

            Ok(())
        }
    }

    struct Ending;

    impl Ending {
        fn new(_: SimpleMessageBox<RuntimeRequest, ()>) -> Self {
            Self
        }
    }

    #[async_trait]
    impl Actor for Ending {
        fn name(&self) -> &str {
            "Ending"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    struct Panic;

    impl Panic {
        fn new(_: SimpleMessageBox<RuntimeRequest, ()>) -> Self {
            Self
        }
    }

    #[async_trait]
    impl Actor for Panic {
        fn name(&self) -> &str {
            "Panic"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            panic!("Oh dear");
        }
    }

    fn create_actor<ActorBuilder, A, Input, Output>(
        actor: ActorBuilder,
    ) -> (mpsc::Sender<Input>, mpsc::Receiver<Output>, RunActor)
    where
        A: Actor,
        ActorBuilder: Fn(SimpleMessageBox<Input, Output>) -> A,
        Input: Message + From<RuntimeRequest>,
        Output: Message,
    {
        let (input_sender, input_receiver) = mpsc::channel(16);
        let (_, signal_receiver) = mpsc::channel(16);
        let (output_sender, output_receiver) = mpsc::channel(16);
        let output_sender = LoggingSender::new("actor".into(), output_sender.into());
        let receiver = LoggingReceiver::new("actor".into(), input_receiver, signal_receiver);
        let actor = actor(SimpleMessageBox::new(receiver, output_sender));
        let actor = RunActor::new(Box::new(actor), Box::new(input_sender.clone()));

        (input_sender, output_receiver, actor)
    }

    fn init() -> (
        mpsc::Sender<RuntimeAction>,
        mpsc::Receiver<RuntimeEvent>,
        RuntimeActor,
    ) {
        // TODO: remove logging or add something smarter because logging is useful
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Trace)
            .try_init();
        let (actions_sender, actions_receiver) = mpsc::channel(16);
        let (events_sender, events_receiver) = mpsc::channel::<RuntimeEvent>(16);
        let ra = RuntimeActor::new(
            actions_receiver,
            Some(Box::new(events_sender)),
            Duration::from_millis(1),
        );
        (actions_sender, events_receiver, ra)
    }

    #[tokio::test]
    async fn should_end_if_channel_is_closed() {
        // Implicit drop of Sender<RuntimeAction>
        let (_, _, ra) = init();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("Runtime actor failed to stop in time after the actions channel was closed"),
            _ = ra.run() => {}
        }
    }

    #[tokio::test]
    async fn should_spawn_actors() {
        let (mut actions_sender, mut events_receiver, ra) = init();
        let (mut input_sender, mut output_receiver, actor) = create_actor(Echo::new);

        input_sender
            .send(EchoMessage::String("actor should have spawned".into()))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(actor))
            .await
            .unwrap();

        let wait_for_messages = async {
            output_receiver.next().await;

            while let Some(event) = events_receiver.next().await {
                if matches!(event, RuntimeEvent::Started { .. }) {
                    return true;
                }
            }

            false
        };

        tokio::select! {
            spawned_actor_event_received = wait_for_messages => assert!(spawned_actor_event_received, "The actor was not spawned"),
            _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("The actor didn't receive the message"),
            _ = ra.run() => panic!("The runtime actor finished unexpectedly")
        };
    }

    #[tokio::test]
    async fn should_handle_actors_finishing_on_their_own() {
        let (mut actions_sender, mut events_receiver, ra) = init();
        let (_, _, actor1) = create_actor(Ending::new);
        let (_, _, actor2) = create_actor(Ending::new);

        actions_sender
            .send(RuntimeAction::Spawn(actor1))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(actor2))
            .await
            .unwrap();

        let wait_for_actors_to_stop = async {
            let mut count = 0;
            while let Some(event) = events_receiver.next().await {
                if matches!(event, RuntimeEvent::Stopped { .. }) {
                    count += 1;
                }

                if count == 2 {
                    break;
                }
            }
        };

        tokio::select! {
            _ = ra.run() => {},
            _ = wait_for_actors_to_stop => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("Actors failed to stop in time")
        }
    }

    #[tokio::test]
    async fn shutdown() {
        let (mut actions_sender, mut events_receiver, ra) = init();
        let (_, _sender1, actor1) = create_actor(Echo::new);
        let (_, _sender2, actor2) = create_actor(Echo::new);

        actions_sender
            .send(RuntimeAction::Spawn(actor1))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(actor2))
            .await
            .unwrap();

        actions_sender.send(RuntimeAction::Shutdown).await.unwrap();

        tokio::select! {
            _ = ra.run() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => panic!("Actors failed to stop in time")
        }

        let mut actor_shutdown_count = 0;

        while let Some(event) = events_receiver.next().await {
            if matches!(event, RuntimeEvent::Stopped { .. }) {
                actor_shutdown_count += 1;
            }
        }

        assert_eq!(
            actor_shutdown_count, 2,
            "The actors were not shut down successfully"
        );
    }

    #[tokio::test]
    async fn actor_panics() {
        let (mut actions_sender, mut events_receiver, ra) = init();
        let (_, _, panic_actor) = create_actor(Panic::new);
        let (mut sender, mut receiver, echo_actor) = create_actor(Echo::new);

        actions_sender
            .send(RuntimeAction::Spawn(panic_actor))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(echo_actor))
            .await
            .unwrap();

        let wait_for_actor_to_panic = async {
            while let Some(event) = events_receiver.next().await {
                match event {
                    RuntimeEvent::Aborted { task, error } if task == "Panic-0" => {
                        return Some(error);
                    }
                    _ => {}
                }
            }
            None
        };

        tokio::spawn(ra.run());

        // The panic is caught by the runtime and an event is sent
        let error = tokio::time::timeout(Duration::from_secs(1), wait_for_actor_to_panic)
            .await
            .expect("Actor to panic in time");
        assert_eq!(
            error.map(|s| s.replace(char::is_numeric, "")), // ignore the task id
            Some("task  panicked with message \"Oh dear\"".to_string())
        );

        // No more message can be sent to the actors: they have been shutdown
        assert!(sender
            .send(EchoMessage::String("hello".into()))
            .await
            .is_err());

        // The actors have been properly shutdown
        assert_eq!(
            receiver.next().await.unwrap(),
            EchoMessage::String("Echo stopped".into())
        );
    }
}
