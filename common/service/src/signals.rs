use core::pin::Pin;
use futures::stream::{SelectAll, Stream};
use futures::task::{Context, Poll};

pub type SignalStream = SelectAll<SignalHandler>;

#[derive(Copy, Clone)]
/// Portable signal kind abstraction.
pub enum SignalKind {
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
        emit_signal: SignalKind,
    },
    #[cfg(windows)]
    GenericSignal {
        receiver: tokio::sync::mpsc::Receiver<()>,
        emit_signal: SignalKind,
    },
}

impl Stream for SignalHandler {
    type Item = SignalKind;

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
    register_hangup: bool,
    register_terminate: bool,
    register_interrupt: bool,
}

impl SignalStreamBuilder {
    pub fn new() -> Self {
        Self {
            register_hangup: true,
            register_terminate: true,
            register_interrupt: true,
        }
    }

    pub fn ignore_sighup(self) -> Self {
        Self {
            register_hangup: false,
            ..self
        }
    }

    pub fn ignore_sigterm(self) -> Self {
        Self {
            register_terminate: true,
            ..self
        }
    }

    pub fn ignore_sigint(self) -> Self {
        Self {
            register_interrupt: true,
            ..self
        }
    }

    #[cfg(not(windows))]
    pub fn build(self) -> std::io::Result<SignalStream> {
        use tokio::signal::unix;

        let mut signals = SelectAll::new();

        if self.register_hangup {
            signals.push(SignalHandler::UnixSignal {
                stream: unix::signal(unix::SignalKind::hangup())?,
                emit_signal: SignalKind::Hangup,
            });
        }

        if self.register_terminate {
            signals.push(SignalHandler::UnixSignal {
                stream: unix::signal(unix::SignalKind::terminate())?,
                emit_signal: SignalKind::Terminate,
            });
        }

        if self.register_interrupt {
            signals.push(SignalHandler::UnixSignal {
                stream: unix::signal(unix::SignalKind::interrupt())?,
                emit_signal: SignalKind::Interrupt,
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

        if self.register_interrupt {
            let (sender, receiver) = tokio::sync::mpsc::channel::<()>(1);
            tokio::spawn(async move {
                if let Ok(_) = tokio::signal::ctrl_c().await {
                    let _ = sender.send(()).await;
                }
            });

            signals.push(SignalHandler::GenericSignal {
                receiver,
                emit_signal: SignalKind::Interrupt,
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
