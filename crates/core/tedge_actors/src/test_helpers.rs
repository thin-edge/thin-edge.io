use crate::mpsc;
use crate::Builder;
use crate::DynSender;
use crate::Message;
use crate::MessageSink;
use crate::MessageSource;
use crate::NoConfig;
use crate::NullSender;
use crate::Sender;
use crate::ServiceConsumer;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use futures::stream::FusedStream;
use futures::SinkExt;
use futures::StreamExt;
use std::fmt::Debug;

/// A message that can be broadcast
pub trait MessagePlus: Message + Clone + Eq {}
impl<T: Message + Clone + Eq> MessagePlus for T {}

pub struct Probe<I: MessagePlus, O: MessagePlus> {
    input_interceptor: mpsc::Sender<I>,
    input_receiver: mpsc::Receiver<I>,
    input_forwarder: DynSender<I>,
    output_interceptor: mpsc::Sender<O>,
    output_receiver: mpsc::Receiver<O>,
    output_forwarder: DynSender<O>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProbeEvent<I: MessagePlus, O: MessagePlus> {
    Recv(I),
    Send(O),
    CloseRecv,
    CloseSend,
    Closed,
}

impl<I: MessagePlus, O: MessagePlus> Default for Probe<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: MessagePlus, O: MessagePlus> Probe<I, O> {
    pub fn new() -> Self {
        // The capacity of the interceptor channels is 1,
        // so the probe will control at which pace input/output messages are sent.
        let (input_interceptor, input_receiver) = mpsc::channel(1);
        let (output_interceptor, output_receiver) = mpsc::channel(1);

        // Use null senders till this probe is connected to actual message boxes.
        let input_forwarder = NullSender.into();
        let output_forwarder = NullSender.into();

        Probe {
            input_interceptor,
            input_receiver,
            input_forwarder,
            output_interceptor,
            output_receiver,
            output_forwarder,
        }
    }

    pub async fn observe(&mut self) -> ProbeEvent<I, O> {
        // Ensure that input/output can only be sent by the observed actors
        let _ = self.input_interceptor.close().await;
        let _ = self.output_interceptor.close().await;

        // Both input and output sender actors might have completed
        if self.input_receiver.is_terminated() && self.output_receiver.is_terminated() {
            return ProbeEvent::Closed;
        }

        // When the input sender has completed: focus on output
        if self.input_receiver.is_terminated() {
            let output = self.output_receiver.next().await;
            return self.notify_output(output).await;
        }

        // When the output sender has completed: focus on input
        if self.output_receiver.is_terminated() {
            let input = self.input_receiver.next().await;
            return self.notify_input(input).await;
        }

        // Notify either input or output depending which is first
        tokio::select! {
            input = self.input_receiver.next() => {
                self.notify_input(input).await
            },
            output = self.output_receiver.next() => {
                self.notify_output(output).await
            },
        }
    }

    async fn notify_input(&mut self, input: Option<I>) -> ProbeEvent<I, O> {
        match input {
            None => ProbeEvent::CloseRecv,
            Some(input) => {
                let event = input.clone();
                self.input_forwarder
                    .send(input)
                    .await
                    .expect("input to be forwarded");
                ProbeEvent::Recv(event)
            }
        }
    }

    async fn notify_output(&mut self, output: Option<O>) -> ProbeEvent<I, O> {
        match output {
            None => ProbeEvent::CloseSend,
            Some(output) => {
                let event = output.clone();
                self.output_forwarder
                    .send(output)
                    .await
                    .expect("output to be forwarded");
                ProbeEvent::Send(event)
            }
        }
    }
}

pub trait ServiceConsumerExt<Request: MessagePlus, Response: MessagePlus> {
    fn with_probe<'a>(
        &'a mut self,
        probe: &'a mut Probe<Response, Request>,
    ) -> &'a mut Probe<Response, Request>;
}

impl<T, Request: MessagePlus, Response: MessagePlus> ServiceConsumerExt<Request, Response> for T
where
    T: ServiceConsumer<Request, Response, NoConfig>,
{
    fn with_probe<'a>(
        &'a mut self,
        probe: &'a mut Probe<Response, Request>,
    ) -> &'a mut Probe<Response, Request> {
        probe.input_forwarder = self.get_response_sender();
        self.set_request_sender(probe.output_interceptor.clone().into());
        probe
    }
}

impl<I: MessagePlus, O: MessagePlus> ServiceConsumer<O, I, NoConfig> for Probe<I, O> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<O>) {
        self.output_forwarder = sender;
    }

    fn get_response_sender(&self) -> DynSender<I> {
        self.input_interceptor.clone().into()
    }
}

impl<I: MessagePlus, O: MessagePlus> MessageSource<O, NoConfig> for Probe<I, O> {
    fn register_peer(&mut self, _config: NoConfig, sender: DynSender<O>) {
        self.output_forwarder = sender;
    }
}

impl<I: MessagePlus, O: MessagePlus> MessageSink<I> for Probe<I, O> {
    fn get_sender(&self) -> DynSender<I> {
        self.input_interceptor.clone().into()
    }
}

pub trait ServiceProviderExt<I: Message, O: Message, C> {
    /// Create a simple message box connected to a box under construction.
    fn new_client_box(&mut self, config: C) -> SimpleMessageBox<O, I>;
}

impl<I, O, C, T> ServiceProviderExt<I, O, C> for T
where
    I: Message,
    O: Message,
    C: Clone,
    T: ServiceProvider<I, O, C>,
{
    fn new_client_box(&mut self, config: C) -> SimpleMessageBox<O, I> {
        let name = "client-box";
        let capacity = 16;
        let mut client_box = ConsumerBoxBuilder::new(name, capacity, config);
        self.connect_with(&mut client_box);
        client_box.build()
    }
}

struct ConsumerBoxBuilder<I, O, C> {
    config: C,
    box_builder: SimpleMessageBoxBuilder<O, I>,
}

impl<I: Message, O: Message, C> ConsumerBoxBuilder<I, O, C> {
    fn new(name: &str, capacity: usize, config: C) -> Self {
        ConsumerBoxBuilder {
            config,
            box_builder: SimpleMessageBoxBuilder::new(name, capacity),
        }
    }

    fn build(self) -> SimpleMessageBox<O, I> {
        self.box_builder.build()
    }
}

impl<I: Message, O: Message, C: Clone> ServiceConsumer<I, O, C> for ConsumerBoxBuilder<I, O, C> {
    fn get_config(&self) -> C {
        self.config.clone()
    }

    fn set_request_sender(&mut self, request_sender: DynSender<I>) {
        self.box_builder.set_request_sender(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<O> {
        self.box_builder.get_response_sender()
    }
}
