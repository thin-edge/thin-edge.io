use nix::unistd::Pid;
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
