use std::io;

#[cfg(not(windows))]
use tokio::signal::unix::{signal, SignalKind};

#[cfg(not(windows))]
pub async fn interrupt() -> io::Result<()> {
    let mut signals = signal(SignalKind::interrupt())?;
    signals.recv().await
}

#[cfg(windows)]
pub async fn interrupt() -> io::Result<()> {
    tokio::signal::ctrl_c().await
}
