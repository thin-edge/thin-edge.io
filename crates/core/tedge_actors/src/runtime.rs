use crate::internal::RunActor;
use crate::internal::Task;
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
use log::debug;
use log::error;
use log::info;
use std::collections::HashMap;
use std::panic;
use std::time::Duration;
use tokio::task::JoinError;
use tokio::task::JoinHandle;

/// Actions sent by actors to the runtime
#[derive(Debug)]
pub enum RuntimeAction {
    Shutdown,
    Spawn(Box<dyn Task>),
}

/// Requests sent by the runtime to actors
#[derive(Clone, Debug)]
pub enum RuntimeRequest {
    Shutdown,
}

/// Events published by the runtime
#[derive(Debug)]
pub enum RuntimeEvent {
    Error(RuntimeError),
    Started { task: String },
    Stopped { task: String },
    Aborted { task: String, error: RuntimeError },
}

/// The actor runtime
pub struct Runtime {
    handle: RuntimeHandle,
    bg_task: JoinHandle<()>,
}

impl Runtime {
    /// Launch the runtime, returning a runtime handler
    ///
    /// TODO: ensure this can only be called once
    pub async fn try_new(
        events_sender: Option<DynSender<RuntimeEvent>>,
    ) -> Result<Runtime, RuntimeError> {
        let (actions_sender, actions_receiver) = mpsc::channel(16);
        let runtime_actor =
            RuntimeActor::new(actions_receiver, events_sender, Duration::from_secs(60));

        let runtime_task = tokio::spawn(runtime_actor.run());
        let runtime = Runtime {
            handle: RuntimeHandle { actions_sender },
            bg_task: runtime_task,
        };
        Ok(runtime)
    }

    pub fn get_handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }

    /// Spawn an actor
    pub async fn spawn<T, A>(&mut self, actor_builder: T) -> Result<(), RuntimeError>
    where
        T: Builder<(A, A::MessageBox)> + RuntimeRequestSink,
        A: Actor,
    {
        let runtime_request_sender: DynSender<RuntimeRequest> = actor_builder.get_signal_sender();
        let (actor, actor_box) = actor_builder.build();
        let run_actor = RunActor::new(actor, actor_box, runtime_request_sender);
        self.handle.spawn(run_actor).await?;
        Ok(())
    }

    /// Run the runtime up to completion
    ///
    /// I.e until
    /// - Either, a `Shutdown` action is sent to the runtime
    /// - Or, all the runtime handler clones have been dropped
    ///       and all the running tasks have reach completion (successfully or not).
    pub async fn run_to_completion(self) -> Result<(), RuntimeError> {
        Runtime::wait_for_completion(self.bg_task).await
    }

    async fn wait_for_completion(bg_task: JoinHandle<()>) -> Result<(), RuntimeError> {
        bg_task.await.map_err(|err| {
            if err.is_panic() {
                RuntimeError::RuntimePanic
            } else {
                RuntimeError::RuntimeCancellation
            }
        })
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

    /// Launch a task in the background
    pub async fn spawn(&mut self, task: impl Task) -> Result<(), RuntimeError> {
        Ok(self.send(RuntimeAction::Spawn(Box::new(task))).await?)
    }

    /// Launch an actor instance
    pub async fn run<A: Actor>(
        &mut self,
        actor: A,
        messages: A::MessageBox,
        runtime_request_sender: DynSender<RuntimeRequest>,
    ) -> Result<(), RuntimeError> {
        self.spawn(RunActor::new(actor, messages, runtime_request_sender))
            .await
    }

    /// Send an action to the runtime
    pub async fn send(&mut self, action: RuntimeAction) -> Result<(), ChannelError> {
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

    async fn run(mut self) {
        info!(target: "Runtime", "Started");
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
                                    self.running_actors.insert(running_name.clone(), actor.runtime_request_sender());
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
                    self.handle_actor_finishing(finished_actor).await;
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(self.cleanup_duration) => error!(target: "Runtime", "Timeout waiting for all actors to shutdown"),
            _ = self.wait_for_actors_to_finish() => info!(target: "Runtime", "All actors have finished")
        }
    }

    async fn wait_for_actors_to_finish(&mut self) {
        while let Some(finished_actor) = self.futures.next().await {
            self.handle_actor_finishing(finished_actor).await;
        }
    }

    async fn handle_actor_finishing(
        &mut self,
        finished_actor: Result<Result<String, (String, RuntimeError)>, JoinError>,
    ) {
        match finished_actor {
            Err(e) => info!(target: "Runtime", "Failed to execute actor: {e}"), //FIXME: this happens on panic in actor
            Ok(Ok(actor)) => {
                self.running_actors.remove(&actor);
                info!(target: "Runtime", "Actor has finished: {actor}");
                self.send_event(RuntimeEvent::Stopped { task: actor }).await;
            }
            Ok(Err((actor, error))) => {
                self.running_actors.remove(&actor);
                error!(target: "Runtime", "Actor has finished unsuccessfully: {actor}");
                self.send_event(RuntimeEvent::Aborted { task: actor, error })
                    .await;
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
                info!(target: "Runtime", "Successfully sent shutdown request to {running_as}")
            }
            Err(e) => {
                error!(target: "Runtime", "Failed to send shutdown request to {running_as}: {e:?}")
            }
        }
    }
}

async fn run_task(
    task: Box<dyn Task>,
    running_name: String,
) -> Result<String, (String, RuntimeError)> {
    task.run().await.map_err(|e| (running_name.clone(), e))?;

    Ok(running_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_in_message_type;
    use crate::Message;
    use crate::SimpleMessageBox;
    use async_trait::async_trait;
    use futures::channel::mpsc;
    use futures::channel::mpsc::Sender;
    use std::time::Duration;

    fan_in_message_type!(EchoMessage[String, RuntimeRequest] : Debug);

    pub struct Echo;

    #[async_trait]
    impl Actor for Echo {
        type MessageBox = SimpleMessageBox<EchoMessage, EchoMessage>;

        fn name(&self) -> &str {
            "Echo"
        }

        async fn run(
            mut self,
            mut messages: SimpleMessageBox<EchoMessage, EchoMessage>,
        ) -> Result<(), ChannelError> {
            // FIXME: If the channel we use to send messages is dropped then we will get an ChannelError::SendError
            // FIXME: but I don't think we shouldn't return this error if the message box has a shutdown message for us
            while let Some(message) = messages.recv().await {
                match message {
                    EchoMessage::String(message) => {
                        messages.send(EchoMessage::String(message)).await?
                    }
                    EchoMessage::RuntimeRequest(RuntimeRequest::Shutdown) => break,
                }
            }

            Ok(())
        }
    }

    struct Ending;

    #[async_trait]
    impl Actor for Ending {
        type MessageBox = SimpleMessageBox<RuntimeRequest, ()>;

        fn name(&self) -> &str {
            "Ending"
        }

        async fn run(
            mut self,
            _: SimpleMessageBox<RuntimeRequest, ()>,
        ) -> Result<(), ChannelError> {
            Ok(())
        }
    }

    // struct Panic;

    // #[async_trait]
    // impl Actor for Panic {
    //     type MessageBox = SimpleMessageBox<(), ()>;

    //     fn name(&self) -> &str {
    //         "Panic"
    //     }

    //     async fn run(mut self, _: SimpleMessageBox<(), ()>) -> Result<(), ChannelError> {
    //         panic!("Oh dear");
    //     }
    // }

    fn create_actor<A, Input, Output>(
        actor: A,
    ) -> (Sender<Input>, mpsc::Receiver<Output>, RunActor<A>)
    where
        A: Actor<MessageBox = SimpleMessageBox<Input, Output>>,
        Input: Message + From<RuntimeRequest>,
        Output: Message,
    {
        let (input_sender, input_receiver) = mpsc::channel(16);
        let (_, signal_receiver) = mpsc::channel(16);
        let (output_sender, output_receiver) = mpsc::channel(16);
        let actor = RunActor::new(
            actor,
            SimpleMessageBox::new(
                "actor".into(),
                input_receiver,
                signal_receiver,
                Box::new(output_sender),
            ),
            Box::new(input_sender.clone()),
        );

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
        let (mut input_sender, mut output_receiver, actor) = create_actor(Echo);

        input_sender
            .send(EchoMessage::String("actor should have spawned".into()))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(Box::new(actor)))
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
        let (_, _, actor1) = create_actor(Ending);
        let (_, _, actor2) = create_actor(Ending);

        actions_sender
            .send(RuntimeAction::Spawn(Box::new(actor1)))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(Box::new(actor2)))
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
        let (_, _, actor1) = create_actor(Echo);
        let (_, _, actor2) = create_actor(Echo);

        actions_sender
            .send(RuntimeAction::Spawn(Box::new(actor1)))
            .await
            .unwrap();

        actions_sender
            .send(RuntimeAction::Spawn(Box::new(actor2)))
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

    // TODO: An actor panic doesn't print the way I want
    // #[tokio::test]
    // async fn actor_panics() {
    //     let (mut actions_sender, mut events_receiver, runtime_request_sender, ra) = init();
    //     let (mut input_sender, mut output_receiver, actor) =
    //         create_actor(Panic {}, &runtime_request_sender);

    //     actions_sender
    //         .send(RuntimeAction::Spawn(Box::new(actor)))
    //         .await
    //         .unwrap();

    //     let wait_for_messages = async {
    //         output_receiver.next().await;

    //         while let Some(event) = events_receiver.next().await {
    //             dbg!(&event);
    //         }
    //         true
    //     };

    //     tokio::select! {
    //         spawned_actor_event_received = wait_for_messages => assert!(spawned_actor_event_received, "The actor was not spawned"),
    //         _ = ra.run() => panic!("The runtime actor finished unexpectedly")
    //     };
    // }
}
