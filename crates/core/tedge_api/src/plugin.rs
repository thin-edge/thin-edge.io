//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use futures::future::BoxFuture;
use std::any::{Any, TypeId};

use async_trait::async_trait;

use crate::error::PluginError;

/// The communication struct to interface with the core of ThinEdge
///
/// It's main purpose is the [`send`](CoreCommunication::send) method, through which one plugin
/// can communicate with another.
#[derive(Clone)]
pub struct CoreCommunication {
    plugin_name: String,
}

impl std::fmt::Debug for CoreCommunication {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Comms")
            .field("plugin_name", &self.plugin_name)
            .finish_non_exhaustive()
    }
}

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

/// A plugin builder for a given plugin
#[async_trait]
pub trait PluginBuilder: Sync + Send + 'static {
    /// The name for the kind of plugins this creates, this should be unique and will prevent startup otherwise
    fn kind_name(&self) -> &'static str;

    /// A list of message types the plugin this builder creates supports
    ///
    /// To create it, you must use the `HandleTypes::get_handlers_for` method. See there on how to
    /// use it.
    fn kind_message_types(&self) -> HandleTypes;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    async fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    async fn instantiate(
        &self,
        config: PluginConfiguration,
        core_comms: CoreCommunication,
    ) -> Result<BuiltPlugin, PluginError>;
}

/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send + std::any::Any {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

#[async_trait]
pub trait Handle<Msg> {
    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: Msg) -> Result<(), PluginError>;
}

#[derive(Debug)]
pub struct HandleTypes(Vec<(&'static str, TypeId)>);

impl HandleTypes {
    /// Get a list of message types this plugin is proven to handle
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use async_trait::async_trait;
    /// # use tedge_api::plugin::{Message, Handle, HandleTypes};
    /// # use tedge_api::PluginError;
    ///
    /// struct Heartbeat;
    ///
    /// impl Message for Heartbeat {}
    ///
    /// struct HeartbeatPlugin;
    ///
    /// #[async_trait]
    /// impl Handle<Heartbeat> for HeartbeatPlugin {
    ///     async fn handle_message(&self, message: Heartbeat) -> Result<(), PluginError> {
    ///     // ... Do something with it
    ///#         Ok(())
    ///     }
    /// }
    ///
    /// # impl Plugin for HeartbeatPlugin {  }
    ///
    /// println!("{:#?}", HandleTypes::get_handlers_for::<(Heartbeat,), HeartbeatPlugin>());
    /// // This will print something akin to:
    /// //
    /// // HandleTypes(
    /// //  [
    /// //      (
    /// //          "rust_out::main::_doctest_main_src_plugin_rs_102_0::Heartbeat",
    /// //          TypeId {
    /// //              t: 15512189350087767644,
    /// //          },
    /// //      ),
    /// //  ],
    /// // )
    /// ```
    ///
    /// Should you ask for messages the plugin does _not_ support, you will receive a compile
    /// error:
    /// ```compile_fail
    /// # use async_trait::async_trait;
    /// # use tedge_api::plugin::{Message, Handle, HandleTypes};
    /// # use tedge_api::PluginError;
    ///
    /// struct Heartbeat;
    ///
    /// impl Message for Heartbeat {}
    ///
    /// struct HeartbeatPlugin;
    ///
    /// // This will fail to compile as the `Heartbeat` message is not handled by the plugin
    /// println!("{:#?}", HandleTypes::get_handlers_for::<(Heartbeat,), HeartbeatPlugin>());
    /// ```
    ///
    /// The error from the rust compiler would look like this (giving a clear indication of what is
    /// missing):
    /// ```text
    /// error[E0277]: the trait bound `HeartbeatPlugin: Handle<Heartbeat>` is not satisfied
    ///    --> src/plugin.rs:XX:YYY
    ///     |
    /// XX  | println!("{:#?}", HandleTypes::get_handlers_for::<(Heartbeat,), HeartbeatPlugin>());
    ///     |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `Handle<Heartbeat>` is not implemented for `HeartbeatPlugin`
    /// ```
    pub fn get_handlers_for<M: MsgBundle, Plugin: DoesHandle<M>>() -> HandleTypes {
        HandleTypes(M::get_ids())
    }
}

pub trait MsgBundle {
    fn get_ids() -> Vec<(&'static str, TypeId)>;
}

impl<A: Message> MsgBundle for (A,) {
    fn get_ids() -> Vec<(&'static str, TypeId)> {
        vec![(std::any::type_name::<A>(), TypeId::of::<A>())]
    }
}

impl<A: Message, B: Message> MsgBundle for (A, B) {
    fn get_ids() -> Vec<(&'static str, TypeId)> {
        vec![
            (std::any::type_name::<A>(), TypeId::of::<A>()),
            (std::any::type_name::<B>(), TypeId::of::<B>()),
        ]
    }
}

type PluginHandlerFn =
    for<'r> fn(&'r dyn Any, Box<dyn Any + Send>) -> BoxFuture<'r, Result<(), PluginError>>;

pub struct BuiltPlugin {
    plugin: Box<dyn Plugin>,
    handler: PluginHandlerFn,
}

impl BuiltPlugin {
    pub fn handle_message(
        &self,
        message: Box<dyn Any + Send>,
    ) -> BoxFuture<'_, Result<(), PluginError>> {
        (self.handler)(&self.plugin, message)
    }
}

pub trait DoesHandle<M: MsgBundle> {
    fn into_untyped(self) -> BuiltPlugin;
}

// TODO: Implement these with a macro to cut down on repetition

impl<A: Message, PLUG: Plugin + Handle<A>> DoesHandle<(A,)> for PLUG {
    fn into_untyped(self) -> BuiltPlugin {
        fn handle_message<'a, M: Message, PLUG: Plugin + Handle<M>>(
            plugin: &'a dyn Any,
            message: Box<dyn Any + Send>,
        ) -> BoxFuture<'a, Result<(), PluginError>> {
            let plug = plugin.downcast_ref::<PLUG>().unwrap();
            let message = {
                if let Ok(message) = message.downcast::<M>() {
                    message
                } else {
                    unreachable!()
                }
            };

            futures::FutureExt::boxed(async move { plug.handle_message(*message).await })
        }
        BuiltPlugin {
            plugin: Box::new(self),
            handler: handle_message::<A, PLUG>,
        }
    }
}
impl<A: Message, B: Message, PLUG: Plugin + Handle<A> + Handle<B>> DoesHandle<(A, B)> for PLUG {
    fn into_untyped(self) -> BuiltPlugin {
        fn handle_message<'a, A: Message, B: Message, PLUG: Plugin + Handle<A> + Handle<B>>(
            plugin: &'a dyn Any,
            message: Box<dyn Any + Send>,
        ) -> BoxFuture<'a, Result<(), PluginError>> {
            let plug = plugin.downcast_ref::<PLUG>().unwrap();
            futures::FutureExt::boxed(async move {
                let message = match message.downcast::<A>() {
                    Ok(message) => return plug.handle_message(*message).await,
                    Err(m) => m,
                };

                match message.downcast::<B>() {
                    Ok(message) => return plug.handle_message(*message).await,
                    Err(m) => m,
                };

                unreachable!();
            })
        }
        BuiltPlugin {
            plugin: Box::new(self),
            handler: handle_message::<A, B, PLUG>,
        }
    }
}

pub trait Message: 'static + Send {}

#[cfg(test)]
mod tests {
    use super::{CoreCommunication, Plugin, PluginBuilder};
    use static_assertions::{assert_impl_all, assert_obj_safe};

    // Object Safety
    assert_obj_safe!(PluginBuilder);
    assert_obj_safe!(Plugin);

    // Sync + Send
    assert_impl_all!(CoreCommunication: Send, Clone);
}
