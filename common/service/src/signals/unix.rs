use super::SignalKind;
use async_stream::stream;
use futures::stream::Stream;
use tokio::signal::unix::{signal, Signal, SignalKind as UnixSignalKind};

pub fn hangup_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    signal(UnixSignalKind::hangup()).map(|signal| stream_from_signal(signal, SignalKind::Hangup))
}

pub fn terminate_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    signal(UnixSignalKind::terminate())
        .map(|signal| stream_from_signal(signal, SignalKind::Terminate))
}

pub fn interrupt_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    signal(UnixSignalKind::interrupt())
        .map(|signal| stream_from_signal(signal, SignalKind::Interrupt))
}

fn stream_from_signal(
    mut signal: Signal,
    signal_kind: SignalKind,
) -> impl Stream<Item = SignalKind> {
    stream! {
        while let Some(()) = signal.recv().await {
            yield signal_kind;
        }
    }
}
