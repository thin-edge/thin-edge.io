use crate::{ActiveActor, Actor, ActorInstance, Producer, Reactor, RuntimeError, Task};
use futures::channel::mpsc;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::UnboundedSender;
use futures::executor::ThreadPool;
use futures::Future;
use futures::{SinkExt, StreamExt};

pub struct ActorRuntime {
    thread_pool: ThreadPool,
    error_sender: UnboundedSender<RuntimeError>,
    task_sender: UnboundedSender<Box<dyn Task>>,
}

impl ActorRuntime {
    pub fn try_new() -> Result<ActorRuntime, RuntimeError> {
        let thread_pool = ThreadPool::new()?;
        let (error_sender, mut error_receiver): (
            UnboundedSender<RuntimeError>,
            UnboundedReceiver<RuntimeError>,
        ) = mpsc::unbounded();
        let (task_sender, mut task_receiver): (
            UnboundedSender<Box<dyn Task>>,
            UnboundedReceiver<Box<dyn Task>>,
        ) = mpsc::unbounded();

        thread_pool.spawn_ok(async move {
            loop {
                futures::select! {
                    maybe_task =  task_receiver.next() => {
                        if let Some(task) = maybe_task {
                            if let Err(error) = task.run().await {
                                eprintln!("Error: {}", error);
                            }
                        } else {
                            break;
                        }

                    }

                    maybe_error = error_receiver.next() => {
                        if let Some(error) = maybe_error {
                            eprintln!("Error: {}", error);
                        } else {
                            break;
                        }
                    }
                }
            }
        });

        Ok(ActorRuntime {
            thread_pool,
            error_sender,
            task_sender,
        })
    }

    /// Launch an actor instance, returning an handle to stop it
    pub async fn run<A: Actor>(&self, instance: ActorInstance<A>) -> ActiveActor<A> {
        let mut mailbox = instance.mailbox;
        let mut recipient = instance.recipient;
        let mut task_sender = self.task_sender.clone();
        let mut error_sender = self.error_sender.clone();

        let input = mailbox.get_address();

        match instance.actor.start().await {
            Ok((source, mut reactor)) => {
                // TODO: to be replaced by a task
                let source_recipient = recipient.clone();
                self.spawn(source.produce_messages(source_recipient));

                self.spawn(async move {
                    while let Some(message) = mailbox.next_message().await {
                        if let Some(task) = reactor.react(message, &mut recipient).await? {
                            task_sender.send(task).await?;
                        }
                    }

                    Ok(())
                });
            }

            Err(error) => {
                let _ = error_sender.send(error).await;
            }
        }

        ActiveActor { input }
    }

    fn spawn<Task>(&self, task: Task)
    where
        Task: 'static + Send + Future<Output = Result<(), RuntimeError>>,
    {
        let mut error_sender = self.error_sender.clone();
        self.thread_pool.spawn_ok(async move {
            if let Err(error) = task.await {
                let _ = error_sender.send(error).await;
            }
        })
    }
}
