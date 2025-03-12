use nix::unistd::Pid;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use tokio::signal::unix;

#[derive(Copy, Clone, Debug)]
pub enum Interruption {
    Timeout,
    Interrupted,
}

pub struct TermSignals {
    sigint: SignalHandler,
    sigquit: SignalHandler,
    sigterm: SignalHandler,
    timeout: TimeoutHandler,
}

impl TermSignals {
    pub fn new(timeout: Option<Duration>) -> TermSignals {
        let sigint = unix::signal(unix::SignalKind::interrupt())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGINT: {err}"))
            .ok()
            .into();
        let sigterm = unix::signal(unix::SignalKind::terminate())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGTERM: {err}"))
            .ok()
            .into();
        let sigquit = unix::signal(unix::SignalKind::quit())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGQUIT: {err}"))
            .ok()
            .into();
        let timeout = timeout.map(tokio::time::sleep).into();

        TermSignals {
            sigint,
            sigquit,
            sigterm,
            timeout,
        }
    }

    pub async fn might_interrupt<F, O>(&mut self, future: F) -> Result<O, Interruption>
    where
        F: Future<Output = O>,
    {
        tokio::select! {
            Some(_) = &mut self.sigint => Err(Interruption::Interrupted),
            Some(_) = &mut self.sigterm => Err(Interruption::Interrupted),
            Some(_) = &mut self.sigquit => Err(Interruption::Interrupted),
            Some(_) = &mut self.timeout => Err(Interruption::Timeout),
            outcome = future => Ok(outcome),
        }
    }
}

struct SignalHandler {
    signal: Option<unix::Signal>,
}

impl Future for SignalHandler {
    type Output = Option<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.signal.as_mut() {
            Some(signal) => signal.poll_recv(cx),
            None => Poll::Ready(None),
        }
    }
}

impl From<Option<unix::Signal>> for SignalHandler {
    fn from(signal: Option<unix::Signal>) -> Self {
        SignalHandler { signal }
    }
}

struct TimeoutHandler {
    timeout: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl Future for TimeoutHandler {
    type Output = Option<()>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.timeout.as_mut() {
            Some(timeout) => timeout.as_mut().poll(cx).map(Some),
            None => Poll::Ready(None),
        }
    }
}

impl From<Option<tokio::time::Sleep>> for TimeoutHandler {
    fn from(timeout: Option<tokio::time::Sleep>) -> Self {
        TimeoutHandler {
            timeout: timeout.map(Box::pin),
        }
    }
}

pub enum Signal {
    SIGTERM,
    SIGKILL,
}

pub fn terminate_process(pid: u32, signal_type: Signal) {
    let pid: Pid = nix::unistd::Pid::from_raw(pid as nix::libc::pid_t);
    match signal_type {
        Signal::SIGTERM => {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGTERM);
        }
        Signal::SIGKILL => {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGKILL);
        }
    }
}
