use core::pin::Pin;
use futures::stream::{SelectAll, Stream};
use futures::task::{Context, Poll};

pub type SignalStream = SelectAll<SignalHandler>;

#[derive(Copy, Clone)]
pub enum Signal {
    /// SIGHUP
    Hangup,
    /// SIGTERM
    Terminate,
    /// SIGINT on POSIX or CTRL-C on Windows
    Interrupt,
}

pub enum SignalHandler {
    Dummy,
    #[cfg(not(windows))]
    UnixSignal {
        stream: tokio::signal::unix::Signal,
        emit_signal: Signal,
    },
    #[cfg(windows)]
    GenericSignal {
        receiver: tokio::sync::mpsc::Receiver<()>,
        emit_signal: Signal,
    },
}

impl Stream for SignalHandler {
    type Item = Signal;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::Dummy => Poll::Pending,
            #[cfg(not(windows))]
            Self::UnixSignal {
                stream,
                emit_signal,
            } => match stream.poll_recv(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Some(())) => Poll::Ready(Some(*emit_signal)),
                Poll::Ready(None) => Poll::Ready(None),
            },
            #[cfg(windows)]
            Self::GenericSignal {
                receiver,
                emit_signal,
            } => match receiver.poll_recv(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Some(())) => Poll::Ready(Some(*emit_signal)),
                Poll::Ready(None) => Poll::Ready(None),
            },
        }
    }
}

pub struct SignalStreamBuilder {
    ignore_hangup: bool,
    ignore_terminate: bool,
    ignore_interrupt: bool,
}

impl SignalStreamBuilder {
    pub fn new() -> Self {
        Self {
            ignore_hangup: false,
            ignore_terminate: false,
            ignore_interrupt: false,
        }
    }

    pub fn ignore_sighup(self) -> Self {
        Self {
            ignore_hangup: true,
            ..self
        }
    }

    pub fn ignore_sigterm(self) -> Self {
        Self {
            ignore_terminate: true,
            ..self
        }
    }

    pub fn ignore_sigint(self) -> Self {
        Self {
            ignore_interrupt: true,
            ..self
        }
    }

    #[cfg(not(windows))]
    pub fn build(self) -> std::io::Result<SignalStream> {
        use tokio::signal::unix::{signal, SignalKind};

        let mut signals = SelectAll::new();

        if !self.ignore_hangup {
            signals.push(SignalHandler::UnixSignal {
                stream: signal(SignalKind::hangup())?,
                emit_signal: Signal::Hangup,
            });
        }

        if !self.ignore_terminate {
            signals.push(SignalHandler::UnixSignal {
                stream: signal(SignalKind::terminate())?,
                emit_signal: Signal::Terminate,
            });
        }

        if !self.ignore_interrupt {
            signals.push(SignalHandler::UnixSignal {
                stream: signal(SignalKind::interrupt())?,
                emit_signal: Signal::Interrupt,
            });
        }

        // Make sure that we have at least one signal handler.
        // Otherwise, the SignalStream is closed.
        if signals.is_empty() {
            signals.push(SignalHandler::Dummy);
        }

        Ok(signals)
    }

    #[cfg(windows)]
    pub fn build(self) -> std::io::Result<SignalStream> {
        let mut signals = SelectAll::new();

        if !self.ignore_interrupt {
            let (sender, receiver) = tokio::sync::mpsc::channel::<()>(1);
            tokio::spawn(async move {
                if let Ok(_) = tokio::signal::ctrl_c().await {
                    let _ = sender.send(()).await;
                }
            });

            signals.push(SignalHandler::GenericSignal {
                receiver,
                emit_signal: Signal::Interrupt,
            });
        }

        // Make sure that we have at least one signal handler.
        // Otherwise, the SignalStream is closed.
        if signals.is_empty() {
            signals.push(SignalHandler::Dummy);
        }

        Ok(signals)
    }
}
