//! Actors that convert each input message into a sequence of output messages
//!
//! A [Converter] :
//! - emits on start a sequence of output messages - the initialization messages
//! - converts each received input message into a sequence of output messages,
//! - emits on shutdown a sequence of output messages - the termination messages.
//!
//! Such a [Converter] is used as an actor, a [ConvertingActor], built and connected to peer actors
//! using a [ConvertingActorBuilder].
//!
//! ```
//! # use std::convert::Infallible;
//! # use crate::tedge_actors::Converter;
//! # use crate::tedge_actors::ConvertingActor;
//! # use crate::tedge_actors::RuntimeError;
//! # use crate::tedge_actors::SimpleMessageBoxBuilder;
//! # use crate::tedge_actors::ServiceConsumer;
//! struct Repeater;
//!
//! impl Converter for Repeater {
//!     type Input = (u8,i32);
//!     type Output = i32;
//!     type Error = Infallible;
//!
//!     fn convert(&mut self, input: &Self::Input) -> Result<Vec<Self::Output>, Self::Error> {
//!         let (n,msg) = *input;
//!         let mut output = vec![];
//!         for _i in 0..n {
//!             output.push(msg);
//!         }
//!         Ok(output)
//!     }
//! }
//!
//! # #[cfg(feature = "test-helpers")]
//! # #[tokio::main]
//! # async fn main() -> Result<(),RuntimeError> {
//! # use std::time::Duration;
//! # use tedge_actors::{Actor, Builder, MessageReceiver, MessageSource, NoConfig, Sender};
//! # use tedge_actors::test_helpers::MessageReceiverExt;
//! let mut actor = ConvertingActor::builder("Repeater", Repeater, NoConfig);
//! let mut test_box = SimpleMessageBoxBuilder::new("Test", 16).with_connection(&mut actor).build().with_timeout(Duration::from_millis(100));
//! tokio::spawn(async move { actor.build().run().await });
//!
//! test_box.send((3, 42)).await?;
//! test_box.assert_received([42,42,42]).await;
//!
//! test_box.send((0, 55)).await?;
//! test_box.send((2, 1234)).await?;
//! test_box.assert_received([1234,1234]).await;
//!
//! assert_eq!(test_box.recv().await, None);
//!
//! # Ok(())
//! # }
//! ```

use crate::Actor;
use crate::Builder;
use crate::DynSender;
use crate::Message;
use crate::MessageReceiver;
use crate::MessageSink;
use crate::MessageSource;
use crate::NoConfig;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::RuntimeRequestSink;
use crate::Sender;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use async_trait::async_trait;
use std::convert::Infallible;

/// An actor that converts each input message into a sequence of output messages
///
pub trait Converter: 'static + Send + Sync {
    /// The input message type
    type Input: Message;

    /// The output message type
    type Output: Message;

    /// The type of conversion error
    ///
    /// The [Converter::convert_error] method is used to convert these errors into output messages
    type Error: std::error::Error + Send + Sync;

    /// Convert an input message into a vector of output messages
    fn convert(&mut self, input: &Self::Input) -> Result<Vec<Self::Output>, Self::Error>;

    /// Make a message from an error
    ///
    /// Simply return the error if fatal and cannot be translated
    fn convert_error(&mut self, error: Self::Error) -> Result<Vec<Self::Output>, Self::Error> {
        Err(error)
    }

    /// Build the list of messages to send on start
    fn init_messages(&mut self) -> Result<Vec<Self::Output>, Self::Error> {
        Ok(vec![])
    }

    /// Build the list of messages to send on shutdown
    fn shutdown_messages(&mut self) -> Result<Vec<Self::Output>, Self::Error> {
        Ok(vec![])
    }
}

/// Make an [Actor] from a [Converter]
pub struct ConvertingActor<C: Converter> {
    name: String,
    converter: C,
    message_box: SimpleMessageBox<C::Input, C::Output>,
}

impl<C: Converter> ConvertingActor<C> {
    pub fn builder<Config>(
        name: &str,
        converter: C,
        config: Config,
    ) -> ConvertingActorBuilder<C, Config> {
        ConvertingActorBuilder::new(name, converter, config)
    }
}

#[async_trait]
impl<C: Converter> Actor for ConvertingActor<C> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let init_messages = self.init_messages()?;
        self.send(init_messages).await?;

        while let Some(input) = self.recv().await {
            let output_messages = self.convert(&input)?;
            self.send(output_messages).await?;
        }

        let shutdown_messages = self.shutdown_messages()?;
        self.send(shutdown_messages).await?;

        Ok(())
    }
}

impl<C: Converter> ConvertingActor<C> {
    fn init_messages(&mut self) -> Result<Vec<C::Output>, RuntimeError> {
        self.converter
            .init_messages()
            .map_err(|err| Box::new(err).into())
    }

    fn convert(&mut self, input: &C::Input) -> Result<Vec<C::Output>, RuntimeError> {
        self.converter
            .convert(input)
            .map_err(|err| Box::new(err).into())
    }

    fn shutdown_messages(&mut self) -> Result<Vec<C::Output>, RuntimeError> {
        self.converter
            .shutdown_messages()
            .map_err(|err| Box::new(err).into())
    }

    async fn recv(&mut self) -> Option<C::Input> {
        self.message_box.recv().await
    }

    async fn send(&mut self, messages: Vec<C::Output>) -> Result<(), RuntimeError> {
        for message in messages {
            self.message_box.send(message).await?
        }
        Ok(())
    }
}

/// Connect and build a [ConvertingActor] wrapping a [Converter] with some `Config`.
///
/// The config can be any value that can be converted into the configuration expected by the message source.
/// For instance, a converter that consumes MQTT messages needs to provide subscription topics.
///
/// ```no_run
/// # use std::convert::Infallible;
/// # use tedge_actors::Builder;
/// # use tedge_actors::Converter;
/// # use tedge_actors::ConvertingActor;
/// # use tedge_actors::MessageSink;
/// # use tedge_actors::MessageSource;
/// # use tedge_actors::NoConfig;
/// # use tedge_actors::ServiceProvider;
/// # #[derive(Debug)]
/// # struct MqttMessage;
/// # #[derive(Clone)]
/// # struct TopicFilter(&'static str);
/// /// A Converter that translates MQTT messages
/// struct MyConverter;
///
/// impl Converter for MyConverter {
///     type Input = MqttMessage;
///     type Output = MqttMessage;
///     type Error = Infallible;
///
///     fn convert(&mut self, input: &Self::Input) -> Result<Vec<Self::Output>, Self::Error> {
///         todo!()
///     }
/// }
///
/// /// Return a converting actor connected to a peer that is both a source and sink of MQTT messages
/// fn new_converter(
///     mut mqtt_builder: impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage, NoConfig>
/// ) -> ConvertingActor<MyConverter> {
///     // Use a builder to connect and build the actor
///     let mut converter_builder = ConvertingActor::builder(
///         "MyConverter",
///         MyConverter,
///         TopicFilter("some/mqtt/topics/#"));
///
///     // Connect this actor as a sink of the mqtt actor, to receive input messages from it
///     converter_builder.add_input(&mut mqtt_builder);
///
///     // Connect the same mqtt actor as a sink of this actor, to send output messages to it
///     converter_builder.add_sink(&mut mqtt_builder);
///
///     // Finally build the actor
///     converter_builder.build()
/// }
/// ```
pub struct ConvertingActorBuilder<C: Converter, Config> {
    name: String,
    converter: C,
    config: Config,
    message_box: SimpleMessageBoxBuilder<C::Input, C::Output>,
}

impl<C: Converter, Config> ConvertingActorBuilder<C, Config> {
    fn new(name: &str, converter: C, config: Config) -> Self {
        ConvertingActorBuilder {
            name: name.to_string(),
            converter,
            config,
            message_box: SimpleMessageBoxBuilder::new(name, 16), // FIXME: capacity should not be hardcoded
        }
    }
}

impl<C: Converter, Config> Builder<ConvertingActor<C>> for ConvertingActorBuilder<C, Config> {
    type Error = Infallible;

    fn try_build(self) -> Result<ConvertingActor<C>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ConvertingActor<C> {
        ConvertingActor {
            name: self.name,
            converter: self.converter,
            message_box: self.message_box.build(),
        }
    }
}

impl<C: Converter, Config> MessageSource<C::Output, NoConfig>
    for ConvertingActorBuilder<C, Config>
{
    fn register_peer(&mut self, config: NoConfig, sender: DynSender<C::Output>) {
        self.message_box.register_peer(config, sender)
    }
}

impl<C: Converter, Config: Clone, SourceConfig> MessageSink<C::Input, SourceConfig>
    for ConvertingActorBuilder<C, Config>
where
    SourceConfig: From<Config>,
{
    fn get_config(&self) -> SourceConfig {
        self.config.clone().into()
    }

    fn get_sender(&self) -> DynSender<C::Input> {
        self.message_box.get_sender()
    }
}

impl<C: Converter, Config> ServiceProvider<C::Input, C::Output, NoConfig>
    for ConvertingActorBuilder<C, Config>
{
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<C::Output>,
    ) -> DynSender<C::Input> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl<C: Converter, Config> RuntimeRequestSink for ConvertingActorBuilder<C, Config> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}
