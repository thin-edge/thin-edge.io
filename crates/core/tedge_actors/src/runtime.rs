use crate::Actor;
use crate::ActorBuilder;
use crate::ChannelError;
use crate::DynSender;
use crate::RunActor;
use crate::RuntimeError;
use crate::Task;
use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;
use log::debug;
use log::info;
use tokio::task::JoinHandle;

/// Actions sent by actors to the runtime
#[derive(Debug)]
pub enum RuntimeAction {
    Shutdown,
    Spawn(Box<dyn Task>),
}

/// Requests sent by the runtime to actors
#[derive(Debug)]
pub enum RuntimeRequest {
    Shutdown,
}

/// Events published by the runtime
#[derive(Clone, Debug)]
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
    /// TODO ensure this can only be called once
    pub async fn try_new(
        events_sender: Option<DynSender<RuntimeEvent>>,
    ) -> Result<Runtime, RuntimeError> {
        let (actions_sender, actions_receiver) = mpsc::channel(16);
        let runtime_actor = RuntimeActor {
            actions: actions_receiver,
            _events: events_sender,
        };
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
    pub async fn spawn(&mut self, actor: impl ActorBuilder) -> Result<(), RuntimeError> {
        actor.spawn(&mut self.handle).await
    }

    /// Run the runtime up to completion
    ///
    /// I.e until
    /// - Either, a `Shutdown` action is sent to the runtime
    /// - Or, all the runtime handler clones have been dropped
    ///       and all the running tasks have reach completion (successfully or not).
    pub async fn run_to_completion(self) -> Result<(), RuntimeError> {
        // FIXME Dropping the handler terminates the runtime too soon
        //       because the actors have currently no sender connected to the runtime.
        let bg_task = self.drop_runtime_handle();
        Runtime::wait_for_completion(bg_task).await
    }

    /// Drop the runtime handle,
    ///
    /// Tell the runtime that no more actions will be sent from this handle
    /// and that new tasks and actors can only be created by already launched actors.
    fn drop_runtime_handle(self) -> JoinHandle<()> {
        self.bg_task
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
    ) -> Result<(), RuntimeError> {
        self.spawn(RunActor::new(actor, messages)).await
    }

    /// Send an action to the runtime
    pub async fn send(&mut self, action: RuntimeAction) -> Result<(), ChannelError> {
        debug!(target: "Runtime", "schedule {:?}", action);
        self.actions_sender.send(action).await?;
        Ok(())
    }
}

/// The actual runtime implementation
struct RuntimeActor {
    actions: mpsc::Receiver<RuntimeAction>,
    _events: Option<DynSender<RuntimeEvent>>,
    // TODO store a join handle for each running task/actor
    // TODO store a sender of RuntimeRequest to each actors
}

impl RuntimeActor {
    async fn run(mut self) {
        info!(target: "Runtime", "started");
        // TODO select next action or next task completion
        while let Some(action) = self.actions.next().await {
            match action {
                RuntimeAction::Shutdown => {
                    break;
                    // TODO send a Shutdown request to each active actor
                    // TODO wait say 60 s, then cancel all tasks still running
                }
                RuntimeAction::Spawn(task) => {
                    info!(target: "Runtime", "spawn {}", task.name());
                    tokio::spawn(task.run());

                    // TODO log a start event
                    // TODO log the end event on success and failure
                    // TODO store a recipient to send messages to the task/actor
                    // TODO store the join_handle : to be able to cancel the task
                }
            }
        }
        info!(target: "Runtime", "stopped");
    }
}
