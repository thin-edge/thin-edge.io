use super::SignalKind;
use async_stream::stream;
use futures::{future::*, stream::Stream};

// Windows does not support hangup signal. Just return a pending stream.
pub fn hangup_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    Ok(pending().into_stream())
}

// Windows does not terminate signal. Just return a pending stream.
pub fn terminate_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    Ok(pending().into_stream())
}

// Use Ctrl-C on Windows
pub fn interrupt_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
    Ok(stream! {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            yield SignalKind::Interrupt;
        }
    })
}
