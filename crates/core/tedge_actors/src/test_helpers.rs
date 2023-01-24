use crate::mpsc;
use crate::DynSender;
use crate::Message;
use crate::MessageBoxPort;
use crate::NullSender;
use crate::Sender;
use crate::SinkExt;
use futures::stream::FusedStream;
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

pub trait MessageBoxPortExt<Request: MessagePlus, Response: MessagePlus> {
    fn with_probe<'a>(
        &'a mut self,
        probe: &'a mut Probe<Response, Request>,
    ) -> &'a mut Probe<Response, Request>;
}

impl<T, Request: MessagePlus, Response: MessagePlus> MessageBoxPortExt<Request, Response> for T
where
    T: MessageBoxPort<Request, Response>,
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

impl<I: MessagePlus, O: MessagePlus> MessageBoxPort<O, I> for Probe<I, O> {
    fn set_request_sender(&mut self, request_sender: DynSender<O>) {
        self.output_forwarder = request_sender;
    }

    fn get_response_sender(&self) -> DynSender<I> {
        self.input_interceptor.clone().into()
    }
}
