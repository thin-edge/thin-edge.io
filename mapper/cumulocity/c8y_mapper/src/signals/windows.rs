//! Signal compatibility layer for Windows

use tokio::sync::mpsc::*;

/// A stream that will never produce any data.
/// Used on Windows to mock signals that do not exist.
pub struct DummySignalStream {
    // We will never send on the `_sender`, but we
    // need to keep it open otherwise the `recv` will
    // "awake".
    _sender: Sender<()>,
    receiver: Receiver<()>,
}

impl DummySignalStream {
    fn new() -> Self {
        let (sender, receiver) = channel(1);
        Self {
            _sender: sender,
            receiver,
        }
    }

    pub async fn recv(&mut self) -> Option<()> {
        self.receiver.recv().await
    }
}

pub fn sighup_stream() -> std::io::Result<DummySignalStream> {
    Ok(DummySignalStream::new())
}

pub fn sigterm_stream() -> std::io::Result<DummySignalStream> {
    Ok(DummySignalStream::new())
}

pub fn sigint_stream() -> std::io::Result<Receiver<()>> {
    let (sender, receiver) = channel::<()>(1);
    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            let _ = sender.send(()).await;
        }
    });
    Ok(receiver)
}
