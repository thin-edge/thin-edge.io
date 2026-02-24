use crate::MetricPoints;
use crate::TEdgeNetDataCollector;
use std::convert::Infallible;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageSink;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;

pub struct TEdgeNetDataCollectorBuilder {
    input: LoggingReceiver<MetricPoints>,
    input_sender: DynSender<MetricPoints>,
    signal_sender: DynSender<RuntimeRequest>,
}

impl Default for TEdgeNetDataCollectorBuilder {
    fn default() -> Self {
        let (input_sender, input_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input = LoggingReceiver::new("NetData".into(), input_receiver, signal_receiver);

        TEdgeNetDataCollectorBuilder {
            input,
            input_sender: input_sender.into(),
            signal_sender: signal_sender.into(),
        }
    }
}

impl MessageSink<MetricPoints> for TEdgeNetDataCollectorBuilder {
    fn get_sender(&self) -> DynSender<MetricPoints> {
        self.input_sender.sender_clone()
    }
}

impl RuntimeRequestSink for TEdgeNetDataCollectorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

impl Builder<TEdgeNetDataCollector> for TEdgeNetDataCollectorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<TEdgeNetDataCollector, Self::Error> {
        Ok(TEdgeNetDataCollector { input: self.input })
    }
}
