use async_trait::async_trait;
use tedge_api::{
    address::{MessageReceiver, ReplySender},
    message::StopCore,
    plugin::{Handle, Message, PluginExt},
    Plugin, PluginError,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

pub struct CoreTask {
    cancellation_token: CancellationToken,
    receiver: MessageReceiver,
    internal_sender: tokio::sync::mpsc::Sender<CoreInternalMessage>,
    internal_receiver: tokio::sync::mpsc::Receiver<CoreInternalMessage>,
}

impl std::fmt::Debug for CoreTask {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("CoreTask").finish_non_exhaustive()
    }
}

impl CoreTask {
    pub fn new(cancellation_token: CancellationToken, receiver: MessageReceiver) -> Self {
        let (internal_sender, internal_receiver) = tokio::sync::mpsc::channel(10);
        Self {
            cancellation_token,
            receiver,
            internal_sender,
            internal_receiver,
        }
    }

    pub(crate) async fn run(mut self) -> crate::errors::Result<()> {
        let running_core = RunningCore {
            sender: self.internal_sender,
        };
        let built_plugin = running_core.into_untyped::<(StopCore,)>();
        let mut receiver_closed = false;

        loop {
            tokio::select! {
                _cancel = self.cancellation_token.cancelled() => {
                    break;
                },

                internal_message = self.internal_receiver.recv() => {
                    match internal_message {
                        msg @ None | msg @ Some(CoreInternalMessage::Stop) => {
                            if msg.is_none() {
                                warn!("Internal core communication stopped");
                            }
                            debug!("Cancelling cancellation token to stop plugins");
                            self.cancellation_token.cancel();
                            debug!("Stopping core");
                            break;
                        }
                    }
                },

                next_message = self.receiver.recv(), if !receiver_closed => {
                    match next_message {
                        Some(msg) => match built_plugin.handle_message(msg).await {
                            Ok(_) => debug!("Core handled message successfully"),
                            Err(e) => warn!("Core failed to handle message: {:?}", e),
                        },

                        None => {
                            receiver_closed = true;
                            debug!("Receiver closed for Core");
                        },
                    }
                }
            }
        }

        Ok(())
    }
}

struct RunningCore {
    sender: tokio::sync::mpsc::Sender<CoreInternalMessage>,
}

enum CoreInternalMessage {
    Stop,
}

#[async_trait]
impl Plugin for RunningCore {
    async fn setup(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}

#[async_trait]
impl Handle<StopCore> for RunningCore {
    async fn handle_message(
        &self,
        _message: StopCore,
        _sender: ReplySender<<StopCore as Message>::Reply>,
    ) -> Result<(), PluginError> {
        let _ = self.sender.send(CoreInternalMessage::Stop).await;
        Ok(())
    }
}
