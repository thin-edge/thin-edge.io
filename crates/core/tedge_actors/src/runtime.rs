use crate::{ActiveActor, Actor, ActorInstance, MailBox, Producer, Reactor, RuntimeError, Sender};
use futures::executor::ThreadPool;
use futures::Future;

pub struct ActorRuntime {
    thread_pool: ThreadPool,
    mailbox: MailBox<RuntimeError>,
}

impl ActorRuntime {
    pub fn try_new() -> Result<ActorRuntime, RuntimeError> {
        let thread_pool = ThreadPool::new()?;
        let mailbox = MailBox::new();
        Ok(ActorRuntime {
            thread_pool,
            mailbox,
        })
    }

    /// Launch an actor instance, returning an handle to stop it
    pub async fn run<A: Actor>(&self, instance: ActorInstance<A>) -> ActiveActor<A> {
        let mut mailbox = instance.mailbox;
        let mut recipient = instance.recipient;
        let event_recipient = recipient.clone();
        let input = mailbox.get_address();

        match instance.actor.start().await {
            Ok((source, mut reactor)) => {
                self.spawn(source.produce_messages(event_recipient));

                self.spawn(async move {
                    while let Some(message) = mailbox.next_message().await {
                        reactor.react(message, &mut recipient).await?;
                    }

                    Ok(())
                });
            }

            Err(error) => {
                let mut error_recipient = self.mailbox.get_address();
                let _ = error_recipient.send_message(error).await;
            }
        }

        ActiveActor { input }
    }

    fn spawn<Task>(&self, task: Task)
    where
        Task: 'static + Send + Future<Output = Result<(), RuntimeError>>,
    {
        let mut error_recipient = self.mailbox.get_address();
        self.thread_pool.spawn_ok(async move {
            if let Err(error) = task.await {
                let _ = error_recipient.send_message(error).await;
            }
        })
    }
}
