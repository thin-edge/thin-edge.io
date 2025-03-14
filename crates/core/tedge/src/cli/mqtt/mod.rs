pub use self::cli::TEdgeMqttCli;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::signal::unix;
use tokio::time;

mod cli;
mod publish;
mod subscribe;

const MAX_PACKET_SIZE: usize = 268435455; // 256 MB

#[derive(Copy, Clone, Debug)]
enum Interruption {
    Timeout,
    Interrupted,
}

struct TermSignals {
    signals: Option<unix::Signal>,
    timeout: Option<Pin<Box<time::Sleep>>>,
}

impl TermSignals {
    fn new(timeout: Option<Duration>) -> TermSignals {
        let signals = unix::signal(unix::SignalKind::interrupt())
            .map_err(|err| eprintln!("failed to set up signal handler: {}", err))
            .ok();
        let timeout = timeout.map(|duration| Box::pin(time::sleep(duration)));
        TermSignals { signals, timeout }
    }

    async fn might_interrupt<F, O>(&mut self, future: F) -> Result<O, Interruption>
    where
        F: Future<Output = O>,
    {
        match (self.timeout.as_mut(), self.signals.as_mut()) {
            (Some(timeout), Some(signals)) => {
                tokio::select! {
                    Some(_) = signals.recv() => Err(Interruption::Interrupted),
                    _ = timeout => Err(Interruption::Timeout),
                    outcome = future => Ok(outcome),
                }
            }

            (None, Some(signals)) => {
                tokio::select! {
                    Some(_) = signals.recv() => Err(Interruption::Interrupted),
                    outcome = future => Ok(outcome),
                }
            }

            (Some(timeout), None) => {
                tokio::select! {
                    _ = timeout => Err(Interruption::Timeout),
                    outcome = future => Ok(outcome),
                }
            }

            (None, None) => Ok(future.await),
        }
    }
}
