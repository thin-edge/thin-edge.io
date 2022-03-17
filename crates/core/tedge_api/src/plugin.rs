//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use futures::future::BoxFuture;
use std::{
    any::{Any, TypeId},
    collections::HashSet,
};

use downcast_rs::{impl_downcast, DowncastSync};

use async_trait::async_trait;

use crate::{error::PluginError, message::CoreMessages, Address};

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
    fn get_address_for<MB: MessageBundle>(&self, name: &str) -> Result<Address<MB>, PluginError>;

    /// Request an `Address` to the core itself. It will only accept messages from the
    /// [`CoreMessages`] bundle.
    fn get_address_for_core(&self, name: &str) -> Result<Address<CoreMessages>, PluginError>;
}

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

/// A plugin builder for a given plugin
#[async_trait]
pub trait PluginBuilder<PD: PluginDirectory>: Sync + Send + 'static {
    /// The name for the kind of plugins this creates, this should be unique and will prevent startup otherwise
    fn kind_name() -> &'static str
    where
        Self: Sized;

    /// A list of message types the plugin this builder creates supports
    ///
    /// To create it, you must use the [`HandleTypes::get_handlers_for`] method.
    fn kind_message_types() -> HandleTypes
    where
        Self: Sized;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    async fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    async fn instantiate(
        &self,
        config: PluginConfiguration,
        core_comms: &PD,
    ) -> Result<BuiltPlugin, PluginError>
    where
        PD: 'async_trait;
}

/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send + DowncastSync {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

impl_downcast!(sync Plugin);

#[async_trait]
#[doc(hidden)]
pub trait Handle<Msg> {
    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: Msg) -> Result<(), PluginError>;
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
    pub fn get_handlers_for<M: MessageBundle, Plugin: DoesHandle<M>>() -> HandleTypes {
        HandleTypes(M::get_ids())
    }
}

impl From<HandleTypes> for HashSet<(&'static str, TypeId)> {
    fn from(ht: HandleTypes) -> Self {
        ht.0.into_iter().collect()
    }
}

pub trait Message: 'static + Send {}

pub trait MessageBundle {
    fn get_ids() -> Vec<(&'static str, TypeId)>;
}

pub trait PluginExt: Plugin {
    fn into_untyped<M: MessageBundle>(self) -> BuiltPlugin
    where
        Self: DoesHandle<M> + Sized,
    {
        self.into_built_plugin()
    }
}

impl<P: Plugin> PluginExt for P {}

type PluginHandlerFn =
    for<'r> fn(&'r dyn Any, Box<dyn Any + Send>) -> BoxFuture<'r, Result<(), PluginError>>;

pub struct BuiltPlugin {
    plugin: Box<dyn Plugin>,
    handler: PluginHandlerFn,
}

impl BuiltPlugin {
    /// Call the plugin with the given types.
    ///
    /// ## Panics
    ///
    /// This method will panic when given a message it does not understand.
    #[must_use]
    pub fn handle_message(
        &self,
        message: Box<dyn Any + Send>,
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

#[doc(hidden)]
pub trait Contains<M: Message> {}

macro_rules! impl_does_handle_tuple {
    () => {};
    ($cur:ident $($rest:tt)*) => {
        impl<$cur: Message, $($rest: Message,)* PLUG: Plugin + Handle<$cur> $(+ Handle<$rest>)*> DoesHandle<($cur, $($rest),*)> for PLUG {
            fn into_built_plugin(self) -> BuiltPlugin {
                fn handle_message<'a, $cur: Message, $($rest: Message,)* PLUG: Plugin + Handle<$cur> $(+ Handle<$rest>)*>(
                    plugin: &'a dyn Any,
                    message: Box<dyn Any + Send>,
                    ) -> BoxFuture<'a, Result<(), PluginError>> {
                    let plug = match plugin.downcast_ref::<PLUG>() {
                        Some(p) => p,
                        None => {
                            panic!("Could not downcast to {}", std::any::type_name::<PLUG>());
                        }
                    };
                    futures::FutureExt::boxed(async move {
                        #![allow(unused)]

                        let message = match message.downcast::<$cur>() {
                            Ok(message) => return plug.handle_message(*message).await,
                            Err(m) => m,
                        };

                        $(
                        let message = match message.downcast::<$rest>() {
                            Ok(message) => return plug.handle_message(*message).await,
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

impl<M: Message> MessageBundle for M {
    fn get_ids() -> Vec<(&'static str, TypeId)> {
        vec![(std::any::type_name::<M>(), TypeId::of::<M>())]
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

#[macro_export]
macro_rules! make_message_bundle {
    ($pu:vis struct $name:ident($($msg:ty),+)) => {
        $pu struct $name;

        impl $crate::plugin::MessageBundle for $name {
            #[allow(unused_parens)]
            fn get_ids() -> Vec<(&'static str, std::any::TypeId)> {
                <($($msg),+) as $crate::plugin::MessageBundle>::get_ids()
            }
        }

        $(impl $crate::plugin::Contains<$msg> for $name {})+
    };
}

#[cfg(test)]
mod tests {
    use super::{Plugin, PluginBuilder};
    use static_assertions::assert_obj_safe;

    // Object Safety
    assert_obj_safe!(PluginBuilder<()>);
    assert_obj_safe!(Plugin);
}
