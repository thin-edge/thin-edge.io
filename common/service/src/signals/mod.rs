use futures::{
    future::pending,
    stream::{SelectAll, Stream, StreamExt},
    FutureExt,
};
use signal_streams::*;

#[cfg_attr(not(windows), path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod signal_streams;

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

        if self.register_hangup {
            signals.push(hangup_stream()?.boxed());
        }

        if self.register_terminate {
            signals.push(terminate_stream()?.boxed());
        }

        if self.register_interrupt {
            signals.push(interrupt_stream()?.boxed());
        }

        // Make sure that we have at least one signal handler.
        // Otherwise, SelectAll panics.
        if signals.is_empty() {
            signals.push(pending().into_stream().boxed());
        }

        Ok(signals)
    }
}
