use crate::{ActiveActor, Actor, ActorInstance, MailBox, Recipient, RuntimeError};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::executor::ThreadPool;
use futures::{Future, SinkExt, StreamExt};

const QUEUE_SIZE: usize = 64;

/// The main runtime handler used to launch actors
pub struct Runtime {
    handler: RuntimeHandler,
    completion_receiver: oneshot::Receiver<()>,
}

impl Runtime {
    /// Create and launched a new runtime
    pub fn try_new() -> Result<Runtime, RuntimeError> {
        let thread_pool = ThreadPool::new()?;
        let (task_sender, task_receiver) = mpsc::channel(QUEUE_SIZE);
        let (completion_sender, completion_receiver) = oneshot::channel();

        let runtime = RuntimeActor::new(thread_pool.clone(), task_receiver, completion_sender);

        let handler = RuntimeHandler { task_sender };

        thread_pool.spawn_ok(runtime.run_to_completion());

        Ok(Runtime {
            handler,
            completion_receiver,
        })
    }

    /// Launch an actor instance
    pub async fn run<A: Actor>(
        &mut self,
        instance: ActorInstance<A>,
    ) -> Result<ActiveActor<A>, RuntimeError> {
        self.handler.run(instance).await
    }

    /// Run all running actors to completion
    pub async fn run_to_completion(self) {
        let completion_receiver = self.drop_handler();
        let _ = completion_receiver.await;
    }

    /// Drop the runtime handler,
    /// so new tasks and actors can only be created by already launched actors
    fn drop_handler(self) -> oneshot::Receiver<()> {
        self.completion_receiver
    }
}

/// A runtime handler passed to actors to launch background tasks
#[derive(Clone)]
pub struct RuntimeHandler {
    task_sender: mpsc::Sender<Box<dyn Task>>,
}

impl RuntimeHandler {
    /// Launch a task in the background
    pub async fn spawn(&mut self, task: impl Task) -> Result<(), RuntimeError> {
        Ok(self.task_sender.send(Box::new(task)).await?)
    }

    /// Launch an actor instance, returning an handle to stop it
    pub async fn run<A: Actor>(
        &mut self,
        instance: ActorInstance<A>,
    ) -> Result<ActiveActor<A>, RuntimeError> {
        let actor = A::try_new(instance.config)?;
        let mailbox = instance.mailbox;
        let input = mailbox.get_address();
        let output = instance.recipient;

        self.spawn(RunActor {
            runtime: self.clone(),
            actor,
            mailbox,
            output,
        })
        .await?;

        Ok(ActiveActor { input })
    }
}

#[async_trait]
pub trait Task: 'static + Send + Sync {
    async fn run(self: Box<Self>) -> Result<(), RuntimeError>;
}

/// The actual runtime
///
/// TODO Add an enum type for all runtime request, i.e. not only Task: actor start, stop, global stop ...
/// TODO Add an enum type for all runtime response: actor launches, errors, terminations ...
struct RuntimeActor {
    thread_pool: ThreadPool,
    task_receiver: mpsc::Receiver<Box<dyn Task>>,
    completion_sender: oneshot::Sender<()>,
    outcome_sender: mpsc::Sender<Result<(), RuntimeError>>,
    outcome_receiver: mpsc::Receiver<Result<(), RuntimeError>>,
}

impl RuntimeActor {
    fn new(
        thread_pool: ThreadPool,
        task_receiver: mpsc::Receiver<Box<dyn Task>>,
        completion_sender: oneshot::Sender<()>,
    ) -> RuntimeActor {
        let (outcome_sender, outcome_receiver) = mpsc::channel(QUEUE_SIZE);

        RuntimeActor {
            thread_pool,
            task_receiver,
            completion_sender,
            outcome_sender,
            outcome_receiver,
        }
    }

    async fn run_to_completion(mut self) {
        loop {
            futures::select! {
                maybe_task = self.task_receiver.next() => {
                    match maybe_task {
                        Some(task) => self.spawn(task.run()),
                        None => {
                            let _ = self.completion_sender.send(());
                            return;
                        },
                    }
                }
                maybe_res = self.outcome_receiver.next() => {
                    if let  Some(Err(err)) = maybe_res {
                        eprintln!("ERROR: {:?}", err)
                    }
                }
            }
        }
    }

    fn spawn<Task>(&self, task: Task)
    where
        Task: 'static + Send + Future<Output = Result<(), RuntimeError>>,
    {
        let mut outcome_sender = self.outcome_sender.clone();
        self.thread_pool.spawn_ok(async move {
            let outcome = task.await;
            let _ = outcome_sender.send(outcome).await;
        })
    }
}

struct RunActor<A: Actor> {
    runtime: RuntimeHandler,
    actor: A,
    mailbox: MailBox<A::Input>,
    output: Recipient<A::Output>,
}

#[async_trait]
impl<A: Actor> Task for RunActor<A> {
    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let mut runtime = self.runtime;
        let mut actor = self.actor;
        let mut mailbox = self.mailbox;
        let mut output = self.output;

        actor.start(runtime.clone(), output.clone()).await?;

        while let Some(message) = mailbox.next_message().await {
            actor.react(message, &mut runtime, &mut output).await?;
        }

        Ok(())
    }
}
