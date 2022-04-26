use crate::{MailBox, Recipient, RuntimeError};
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

    pub fn spawn<Task>(&self, task: Task)
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
