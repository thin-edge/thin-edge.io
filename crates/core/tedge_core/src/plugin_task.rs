use futures::FutureExt;
use tedge_api::address::MessageReceiver;
use tedge_api::plugin::BuiltPlugin;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::errors::Result;
use crate::errors::TedgeApplicationError;
use crate::task::Task;

pub struct PluginTask {
    plugin_name: String,
    plugin: BuiltPlugin,
    plugin_msg_receiver: MessageReceiver,
    task_cancel_token: CancellationToken,
    shutdown_timeout: std::time::Duration,
}

impl std::fmt::Debug for PluginTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginTask")
            .field("plugin_name", &self.plugin_name)
            .finish()
    }
}

impl PluginTask {
    pub fn new(
        plugin_name: String,
        plugin: BuiltPlugin,
        plugin_msg_receiver: MessageReceiver,
        task_cancel_token: CancellationToken,
        shutdown_timeout: std::time::Duration,
    ) -> Self {
        Self {
            plugin_name,
            plugin,
            plugin_msg_receiver,
            task_cancel_token,
            shutdown_timeout,
        }
    }
}

#[async_trait::async_trait]
impl Task for PluginTask {
    #[tracing::instrument]
    async fn run(mut self) -> Result<()> {
        trace!("Setup for plugin '{}'", self.plugin_name);

        // we can use AssertUnwindSafe here because we're _not_ using the plugin after a panic has
        // happened.
        match std::panic::AssertUnwindSafe(self.plugin.plugin_mut().setup())
            .catch_unwind()
            .await
        {
            Err(_) => {
                // don't make use of the plugin for unwind safety reasons, and the plugin
                // will be dropped

                error!("Plugin {} paniced in setup", self.plugin_name);
                return Err(TedgeApplicationError::PluginSetupPaniced(self.plugin_name));
            }
            Ok(res) => res?,
        }
        trace!("Setup for plugin '{}' finished", self.plugin_name);
        let mut receiver_closed = false;

        trace!("Mainloop for plugin '{}'", self.plugin_name);
        loop {
            tokio::select! {
                next_message = self.plugin_msg_receiver.recv(), if !receiver_closed => {
                    match next_message {
                        Some(msg) => match std::panic::AssertUnwindSafe(self.plugin.handle_message(msg)).catch_unwind().await {
                            Err(_) => {
                                // panic happened in handle_message() implementation

                                error!("Plugin {} paniced in message handler", self.plugin_name);
                                return Err(TedgeApplicationError::PluginMessageHandlerPaniced(self.plugin_name));
                            },
                            Ok(Ok(_)) => debug!("Plugin handled message successfully"),
                            Ok(Err(e)) => warn!("Plugin failed to handle message: {:?}", e),
                        },

                        None => {
                            receiver_closed = true;
                            debug!("Receiver closed for {} plugin", self.plugin_name);
                        },
                    }
                }

                _shutdown = self.task_cancel_token.cancelled() => {
                    // no communication happened when we got this future returned,
                    // so we're done now
                    debug!("Received shutdown request");
                    info!("Going to shut down {}", self.plugin_name);
                    break
                }
            }
        }
        trace!("Mainloop for plugin '{}' finished", self.plugin_name);

        info!("Shutting down {}", self.plugin_name);
        let shutdown_fut = tokio::spawn(async move { self.plugin.plugin_mut().shutdown().await });

        match tokio::time::timeout(self.shutdown_timeout, shutdown_fut).await {
            Err(_timeout) => {
                error!("Shutting down {} timeouted", self.plugin_name);
                Err(TedgeApplicationError::PluginShutdownTimeout(
                    self.plugin_name,
                ))
            }
            Ok(Err(e)) => {
                error!("Waiting for plugin {} shutdown failed", self.plugin_name);
                if e.is_panic() {
                    error!("Shutdown of {} paniced", self.plugin_name);
                } else if e.is_cancelled() {
                    error!("Shutdown of {} cancelled", self.plugin_name);
                }
                Err(TedgeApplicationError::PluginShutdownError(self.plugin_name))
            }
            Ok(Ok(res)) => {
                info!("Shutting down {} completed", self.plugin_name);
                res.map_err(TedgeApplicationError::from)
            }
        }
    }
}
