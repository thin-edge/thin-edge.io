pub use self::cli::TEdgeMqttCli;
pub use self::error::MqttError;
use std::future::Future;
use std::time::Duration;
use tokio::sync::watch;

mod cli;
mod error;
mod publish;
mod subscribe;

const MAX_PACKET_SIZE: usize = 268435455; // 256 MB

#[derive(Copy, Clone, Debug)]
enum Interruption {
    Timeout,
    Interrupted,
    Error,
}

struct TermSignals(watch::Receiver<Option<Interruption>>);

impl TermSignals {
    fn new(timeout: Option<Duration>) -> TermSignals {
        let (tx, rx) = watch::channel(None);
        for signal in signal_hook::consts::TERM_SIGNALS {
            let signal_tx = tx.clone();
            unsafe {
                let _ = signal_hook::low_level::register(*signal, move || {
                    let _ = signal_tx.send(Some(Interruption::Interrupted));
                });
            }
        }

        if let Some(timeout) = timeout {
            std::thread::spawn(move || {
                std::thread::sleep(timeout);
                let _ = tx.send(Some(Interruption::Timeout));
            });
        }

        TermSignals(rx)
    }

    async fn is_interrupted(&mut self) -> Interruption {
        match self.0.wait_for(Option::is_some).await {
            Ok(interruption) => *interruption.as_ref().unwrap(),
            Err(_) => Interruption::Error,
        }
    }

    async fn might_interrupt<F, O>(&mut self, future: F) -> Result<O, Interruption>
    where
        F: Future<Output = O>,
    {
        tokio::select! {
            interruption = self.is_interrupted() => Err(interruption),
            outcome = future => Ok(outcome),
        }
    }
}
