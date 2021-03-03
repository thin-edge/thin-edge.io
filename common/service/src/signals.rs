use futures::stream::StreamExt;
use futures::stream::{SelectAll, Stream};
use futures::FutureExt;

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

#[cfg(not(windows))]
mod unix {
    use super::SignalKind;
    use async_stream::stream;
    use futures::stream::Stream;
    use tokio::signal::unix::{signal, Signal, SignalKind as UnixSignalKind};

    pub fn hangup_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
        signal(UnixSignalKind::hangup())
            .map(|signal| stream_from_signal(signal, SignalKind::Hangup))
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
}

#[cfg(windows)]
mod windows {
    use super::SignalKind;
    use async_stream::stream;
    use futures::stream::Stream;

    pub fn interrupt_stream() -> std::io::Result<impl Stream<Item = SignalKind>> {
        Ok(stream! {
            if let Ok(_) = tokio::signal::ctrl_c().await {
                yield SignalKind::Interrupt;
            }
        })
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

    pub fn build(self) -> std::io::Result<impl Stream<Item = SignalKind> + std::marker::Unpin> {
        let mut signals = SelectAll::new();

        #[cfg(not(windows))]
        if self.register_hangup {
            signals.push(unix::hangup_stream()?.boxed());
        }

        #[cfg(not(windows))]
        if self.register_terminate {
            signals.push(unix::terminate_stream()?.boxed());
        }

        #[cfg(not(windows))]
        if self.register_interrupt {
            signals.push(unix::interrupt_stream()?.boxed());
        }

        #[cfg(windows)]
        if self.register_interrupt {
            signals.push(windows::interrupt_stream()?.boxed());
        }

        // Make sure that we have at least one signal handler.
        // Otherwise, SelectAll panics.
        if signals.is_empty() {
            signals.push(futures::future::pending().into_stream().boxed());
        }

        Ok(signals.boxed())
    }
}
