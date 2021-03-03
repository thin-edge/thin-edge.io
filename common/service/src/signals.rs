use core::pin::Pin;
use futures::stream::StreamExt;
use futures::stream::{SelectAll, Stream};
use futures::task::{Context, Poll};
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

pub struct SignalStream(Pin<Box<dyn Stream<Item = SignalKind>>>);

impl Stream for SignalStream {
    type Item = SignalKind;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = Pin::into_inner(self);
        Pin::new(&mut me.0).poll_next(cx)
    }
}

#[cfg(not(windows))]
mod unix {
    use core::pin::Pin;
    use futures::stream::Stream;
    use futures::task::{Context, Poll};
    use tokio::signal::unix::{signal, Signal, SignalKind};

    pub struct UnixSignalStream(Signal);

    impl Stream for UnixSignalStream {
        type Item = ();

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.get_mut().0.poll_recv(cx)
        }
    }

    impl UnixSignalStream {
        pub fn hangup() -> std::io::Result<Self> {
            signal(SignalKind::hangup()).map(Self)
        }

        pub fn terminate() -> std::io::Result<Self> {
            signal(SignalKind::terminate()).map(Self)
        }

        pub fn interrupt() -> std::io::Result<Self> {
            signal(SignalKind::interrupt()).map(Self)
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

    pub fn build(self) -> std::io::Result<SignalStream> {
        let mut signals = SelectAll::new();

        #[cfg(not(windows))]
        if self.register_hangup {
            signals.push(
                unix::UnixSignalStream::hangup()?
                    .map(|()| SignalKind::Hangup)
                    .boxed(),
            );
        }

        #[cfg(not(windows))]
        if self.register_terminate {
            signals.push(
                unix::UnixSignalStream::terminate()?
                    .map(|()| SignalKind::Terminate)
                    .boxed(),
            );
        }

        #[cfg(not(windows))]
        if self.register_interrupt {
            signals.push(
                unix::UnixSignalStream::interrupt()?
                    .map(|()| SignalKind::Interrupt)
                    .boxed(),
            );
        }

        #[cfg(windows)]
        if self.register_interrupt {
            let (sender, receiver) = tokio::sync::mpsc::channel::<()>(1);
            tokio::spawn(async move {
                if let Ok(_) = tokio::signal::ctrl_c().await {
                    let _ = sender.send(()).await;
                }
            });

            signals.push(
                tokio_stream::wrappers::ReceiverStream::new(receiver)
                    .map(|()| SignalKind::Interrupt)
                    .boxed(),
            );
        }

        // Make sure that we have at least one signal handler.
        // Otherwise, SelectAll panics.
        if signals.is_empty() {
            signals.push(futures::future::pending().into_stream().boxed());
        }

        Ok(SignalStream(signals.boxed()))
    }
}
