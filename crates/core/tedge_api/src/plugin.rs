//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;
use std::{
    any::{Any, TypeId},
    collections::HashSet,
};

use downcast_rs::{impl_downcast, DowncastSync};

use async_trait::async_trait;

use crate::{
    address::{InternalMessage, ReceiverBundle, ReplySender},
    error::PluginError,
    message::CoreMessages,
    Address,
};

/// The communication struct to interface with the core of ThinEdge
///
/// Implementors of this trait can be used to get an address of a certain plugin, which can then be
/// used to send messages of a specific type to that plugin.
/// Alternatively, implementors of this trait can be used to send messages to the core of
/// thin-edge.
///
/// # Note
///
/// As a plugin author, you will not have to implement this trait.
/// The core of thin-edge will use this trait to hand over an object to a plugin that can then be
/// used to communicate with other plugins (as described above).
///
pub trait PluginDirectory: Clone + Send + Sync {
    /// Request an `Address` object for a given plugin which can receive messages of a type
    /// included in the message bundle `MB`.
    ///
    /// ## Also see
    ///
    /// - [`make_message_bundle`] On how to define your own named message bundle
    fn get_address_for<RB: ReceiverBundle>(&self, name: &str) -> Result<Address<RB>, PluginError>;

    /// Request an `Address` to the core itself. It will only accept messages from the
    /// [`CoreMessages`] bundle.
    fn get_address_for_core(&self) -> Result<Address<CoreMessages>, PluginError>;
}

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

/// A plugin builder for a given plugin
///
/// A type implementing PluginBuilder is used by the core of thin-edge to instantiate a plugin
/// implementation.
///
/// # Note
///
/// Plugin authors want to implement this trait so that the core of thin-edge can instantiate their
/// plugin if the configuration of thin-edge desires so.
///
/// The implementation of the trait is then used by thin-edge to verify that the configuration
/// passed to the plugin is sound (what "sound" means in this context is to be decided by the
/// plugin author, i.e. the author of the implementation of this trait).
///
/// The plugin author must also name all message types the plugin which is about to be instantiated
/// can receive (see `PluginBuilder::kind_message_types`).
#[async_trait]
pub trait PluginBuilder<PD: PluginDirectory>: Sync + Send + 'static {
    /// The name for the kind of plugins this creates, this should be unique and will prevent startup otherwise
    ///
    /// The "kind name" of a plugin is used by the configuration to name what plugin is to be
    /// instantiated. For example, if the configuration asks thin-edge to instantiate a plugin
    /// of kind "foo", but only a plugin implementation of kind "bar" is compiled into thin-edge,
    /// the software is able to report misconfiguration on startup.
    fn kind_name() -> &'static str
    where
        Self: Sized;

    /// A list of message types the plugin this builder creates supports
    ///
    /// This function must return a `HandleTypes` object which represents the types of messages
    /// that a plugin is able to handle.
    ///
    /// To create an instance of this type, you must use the [`HandleTypes::get_handlers_for`] method.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use tedge_api::{Message, Plugin, plugin::{BuiltPlugin}, PluginConfiguration}
    ///
    /// #[derive(Debug)]
    /// struct MyMessage;
    /// impl Message for MyMessage {}
    ///
    /// struct MyPluginBuilder;
    /// struct MyPlugin; // + some impl Plugin for MyPlugin
    /// # impl Plugin for MyPlugin {
    /// #     async fn setup(&mut self) -> Result<(), PluginError> {
    /// #         unimplemented!()
    /// #     }
    /// #     async fn shutdown(&mut self) -> Result<(), PluginError> {
    /// #         unimplemented!()
    /// #     }
    /// # }
    ///
    /// #[async_trait]
    /// impl<PD: PluginDirectory> PluginBuilder<PD> for MyPluginBuilder {
    ///     fn kind_message_types() -> tedge_api::plugin::HandleTypes
    ///     where
    ///         Self: Sized,
    ///     {
    ///         HandleTypes::get_handlers_for::<(MyMessage,), MyPlugin>()
    ///     }
    ///     // other trait functions...
    /// #   fn kind_name(&self) -> &'static str {
    /// #       unimplemented!()
    /// #   }
    /// #   async fn verify_configuration(
    /// #       &self,
    /// #       _config: &PluginConfiguration,
    /// #   ) -> Result<(), tedge_api::error::PluginError> {
    /// #       unimplemented!()
    /// #   }
    /// #   async fn instantiate(
    /// #       &self,
    /// #       config: PluginConfiguration,
    /// #       tedge_comms: &PD,
    /// #   ) -> Result<BuiltPlugin, PluginError>
    /// #   where
    /// #       PD: 'async_trait,
    /// #   {
    /// #       unimplemented!()
    /// #   }
    /// }
    /// ```
    fn kind_message_types() -> HandleTypes
    where
        Self: Sized;

    /// Verify the configuration of the plugin for this plugin kind
    ///
    /// This function will be used by the core implementation to verify that a given plugin
    /// configuration can be used by a plugin.
    ///
    /// After the plugin configuration got loaded and deserialized, it might still contain settings
    /// which are erroneous, for example
    ///
    /// ```toml
    /// timeout = -1
    /// ```
    ///
    /// This function can be used by plugin authors to verify that a given verification is sound,
    /// before the plugins are instantiated (to be able to fail early).
    ///
    /// # Note
    ///
    /// This may be called anytime (also while plugins are already running) to verify whether a
    /// plugin could be instantiated with the passed configuration.
    async fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This function is called by the core of thin-edge to create a new plugin instance.
    ///
    /// The `PluginExt::into_untyped()` function can be used to make any `Plugin` implementing type
    /// into a `BuiltPlugin`, which the function requires to be returned (see example below).
    ///
    /// # Note
    ///
    /// This function _must not_ block.
    ///
    /// # Example
    ///
    /// ```no_run
    /// #[derive(Debug)]
    /// struct MyMessage;
    /// impl Message for MyMessage {}
    ///
    /// struct MyPluginBuilder;
    /// struct MyPlugin; // + some impl Plugin for MyPlugin
    /// # impl Plugin for MyPlugin {
    /// #     async fn setup(&mut self) -> Result<(), PluginError> {
    /// #         unimplemented!()
    /// #     }
    /// #     async fn shutdown(&mut self) -> Result<(), PluginError> {
    /// #         unimplemented!()
    /// #     }
    /// # }
    ///
    /// #[async_trait]
    /// impl<PD: PluginDirectory> PluginBuilder<PD> for MyPluginBuilder {
    ///     async fn instantiate(
    ///         &self,
    ///         config: PluginConfiguration,
    ///         tedge_comms: &PD,
    ///     ) -> Result<BuiltPlugin, PluginError>
    ///     where
    ///         PD: 'async_trait,
    ///     {
    ///         let p = MyPlugin {};
    ///         Ok(p.into_untyped::<(MyMessage,)>())
    ///     }
    ///     // other trait functions...
    /// #   fn kind_name(&self) -> &'static str {
    /// #       unimplemented!()
    /// #   }
    /// #   fn kind_message_types() -> tedge_api::plugin::HandleTypes
    /// #   where
    /// #       Self: Sized,
    /// #   {
    /// #       HandleTypes::get_handlers_for::<(MyMessage,), MyPlugin>()
    /// #   }
    /// #   async fn verify_configuration(
    /// #       &self,
    /// #       _config: &PluginConfiguration,
    /// #   ) -> Result<(), tedge_api::error::PluginError> {
    /// #       unimplemented!()
    /// #   }
    /// }
    /// ```
    async fn instantiate(
        &self,
        config: PluginConfiguration,
        cancellation_token: CancellationToken,
        core_comms: &PD,
    ) -> Result<BuiltPlugin, PluginError>
    where
        PD: 'async_trait;
}

/// A functionality extension to ThinEdge
///
/// The `Plugin` trait can be implemented to implement behaviour within thin-edge.
/// If a plugin also would like to receive messages, the author of a plugin must also implement the
/// `Handle` trait.
///
/// The functionality implemented via the `Plugin` trait is used to setup the plugin before
/// messages are sent to it, and to shut the plugin down when thin-edge stops operation.
///
/// The `Plugin::setup` function would be the place where a plugin author would create background
/// tasks, if their plugin must send messages pro-actively.
#[async_trait]
pub trait Plugin: Sync + Send + DowncastSync {
    /// The plugin can set itself up here
    ///
    /// This function will be called by the core of thin-edge before any message-passing starts.
    /// The plugin is free to for example spawn up background tasks here.
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    ///
    /// This function is called by the core of thin-edge before the software shuts down as a whole,
    /// giving the plugin the opportunity to clear up resources (e.g. deallocate file handles
    /// cleanly, shut down network connections properly, etc...).
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

impl_downcast!(sync Plugin);

/// A trait marking that a plugin is able to handle certain messages
///
/// This trait can be used by plugin authors to make their plugins able to handle messages of a
/// certain type (`Msg`).
///
/// A Plugin that is able to receive different types of messages would have multiple
/// implementations of this trait.
#[async_trait]
pub trait Handle<Msg: Message> {
    /// Handle a message of type `Msg` that gets send to this plugin
    async fn handle_message(
        &self,
        message: Msg,
        sender: ReplySender<Msg::Reply>,
    ) -> Result<(), PluginError>;
}

#[derive(Debug)]
#[doc(hidden)]
pub struct HandleTypes(Vec<(&'static str, TypeId)>);

impl HandleTypes {
    pub fn get_types(&self) -> &[(&'static str, TypeId)] {
        &self.0
    }

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
    pub fn declare_handlers_for<M: MessageBundle, Plugin: DoesHandle<M>>() -> HandleTypes {
        HandleTypes(M::get_ids())
    }

    /// Empty list of types. A plugin that does not handle anything will not be able to receive
    /// messages except through replies sent with [`Reply`](crate::address::Reply)
    pub fn empty() -> HandleTypes {
        HandleTypes(Vec::with_capacity(0))
    }
}

impl From<HandleTypes> for HashSet<(&'static str, TypeId)> {
    fn from(ht: HandleTypes) -> Self {
        ht.0.into_iter().collect()
    }
}

/// A thing that can be send around
///
/// This trait is a marker trait for all types that can be used as messages which can be send
/// between plugins in thin-edge.
pub trait Message: 'static + Send + std::fmt::Debug {
    type Reply: Message;
}

/// A bundle of messages
///
/// This trait is implemented on types that represent a bundle of different types of messages.
pub trait MessageBundle {
    /// Get the names and ids of the types that are represented by this bundle
    fn get_ids() -> Vec<(&'static str, TypeId)>;
}

/// An extension for a Plugin implementing type
///
/// This trait implements an extension for all types that implement `Plugin`.
/// This extension can be used by plugin authors to make their specific plugin type instance into a
/// [`BuiltPlugin`].
pub trait PluginExt: Plugin {
    /// Convert a `Plugin` into a `BuiltPlugin`
    ///
    /// This function is only available if the Plugin is able to handle messages that are inside
    /// the specified `MessageBundle`.
    fn into_untyped<M: MessageBundle>(self) -> BuiltPlugin
    where
        Self: DoesHandle<M> + Sized,
    {
        self.into_built_plugin()
    }
}

impl<P: Plugin> PluginExt for P {}

type PluginHandlerFn =
    for<'r> fn(&'r dyn Any, InternalMessage) -> BoxFuture<'r, Result<(), PluginError>>;

/// A plugin that is instantiated
///
/// This type represents a plugin that is instantiated (via the [`PluginBuilder`]).
#[allow(missing_debug_implementations)]
pub struct BuiltPlugin {
    plugin: Box<dyn Plugin>,
    handler: PluginHandlerFn,
}

impl BuiltPlugin {
    pub fn new(plugin: Box<dyn Plugin>, handler: PluginHandlerFn) -> Self {
        Self { plugin, handler }
    }

    /// Call the plugin with the given types.
    ///
    /// ## Panics
    ///
    /// This method will panic when given a message it does not understand.
    #[must_use]
    pub fn handle_message(
        &self,
        message: InternalMessage,
    ) -> BoxFuture<'_, Result<(), PluginError>> {
        (self.handler)((&*self.plugin).as_any(), message)
    }

    /// Get a mutable reference to the built plugin's plugin.
    pub fn plugin_mut(&mut self) -> &mut Box<dyn Plugin> {
        &mut self.plugin
    }

    /// Get a reference to the built plugin's plugin.
    pub fn plugin(&self) -> &dyn Plugin {
        self.plugin.as_ref()
    }
}

#[doc(hidden)]
pub trait DoesHandle<M: MessageBundle> {
    fn into_built_plugin(self) -> BuiltPlugin;
}

macro_rules! impl_does_handle_tuple {
    () => {};
    ($cur:ident $($rest:tt)*) => {
        impl<$cur: Message, $($rest: Message,)* PLUG: Plugin + Handle<$cur> $(+ Handle<$rest>)*> DoesHandle<($cur, $($rest),*)> for PLUG {
            fn into_built_plugin(self) -> BuiltPlugin {
                fn handle_message<'a, $cur: Message, $($rest: Message,)* PLUG: Plugin + Handle<$cur> $(+ Handle<$rest>)*>(
                    plugin: &'a dyn Any,
                    message: InternalMessage,
                    ) -> BoxFuture<'a, Result<(), PluginError>> {
                    let plug = match plugin.downcast_ref::<PLUG>() {
                        Some(p) => p,
                        None => {
                            panic!("Could not downcast to {}", std::any::type_name::<PLUG>());
                        }
                    };
                    futures::FutureExt::boxed(async move {
                        #![allow(unused)]

                        let InternalMessage { data: message, reply_sender } = message;


                        let message = match message.downcast::<$cur>() {
                            Ok(message) => {
                                let reply_sender = crate::address::ReplySender::new(reply_sender);
                                return plug.handle_message(*message, reply_sender).await
                            }
                            Err(m) => m,
                        };

                        $(
                        let message = match message.downcast::<$rest>() {
                            Ok(message) => {
                                let reply_sender = crate::address::ReplySender::new(reply_sender);
                                return plug.handle_message(*message, reply_sender).await
                            }
                            Err(m) => m,
                        };
                        )*

                        unreachable!();
                    })
                }
                BuiltPlugin {
                    plugin: Box::new(self),
                    handler: handle_message::<$cur, $($rest,)* PLUG>,
                }
            }
        }

        impl_does_handle_tuple!($($rest)*);
    };
}

impl MessageBundle for () {
    fn get_ids() -> Vec<(&'static str, TypeId)> {
        vec![]
    }
}

impl<P: Plugin> DoesHandle<()> for P {
    fn into_built_plugin(self) -> BuiltPlugin {
        fn handle_message<'a, PLUG: Plugin>(
            _plugin: &'a dyn Any,
            _message: InternalMessage,
        ) -> BoxFuture<'a, Result<(), PluginError>> {
            unreachable!()
        }
        BuiltPlugin {
            plugin: Box::new(self),
            handler: handle_message::<P>,
        }
    }
}

macro_rules! impl_msg_bundle_tuple {
    () => {};
    (@rec_tuple $cur:ident) => {
        ($cur, ())
    };
    (@rec_tuple $cur:ident $($rest:tt)*) => {
        ($cur, impl_msg_bundle_tuple!(@rec_tuple $($rest)*))
    };
    ($cur:ident $($rest:tt)*) => {
        impl<$cur: Message, $($rest: Message),*> MessageBundle for ($cur,$($rest),*) {
            fn get_ids() -> Vec<(&'static str, TypeId)> {
                vec![
                    (std::any::type_name::<$cur>(), TypeId::of::<$cur>()),
                    $((std::any::type_name::<$rest>(), TypeId::of::<$rest>())),*
                ]
            }
        }

        impl_msg_bundle_tuple!($($rest)*);
    };
}

impl_msg_bundle_tuple!(M10 M9 M8 M7 M6 M5 M4 M3 M2 M1);
impl_does_handle_tuple!(M10 M9 M8 M7 M6 M5 M4 M3 M2 M1);

#[cfg(test)]
mod tests {
    use super::{Plugin, PluginBuilder};
    use static_assertions::assert_obj_safe;

    // Object Safety
    assert_obj_safe!(PluginBuilder<()>);
    assert_obj_safe!(Plugin);
}
