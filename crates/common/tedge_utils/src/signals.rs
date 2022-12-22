use std::io;

#[cfg(not(windows))]
use tokio::signal::unix::signal;
#[cfg(not(windows))]
use tokio::signal::unix::SignalKind;

#[cfg(not(windows))]
pub async fn interrupt() -> io::Result<()> {
    let mut signals = signal(SignalKind::interrupt())?;
    let _ = signals.recv().await;
    Ok(())
}

#[cfg(windows)]
pub async fn interrupt() -> io::Result<()> {
    tokio::signal::ctrl_c().await
}
