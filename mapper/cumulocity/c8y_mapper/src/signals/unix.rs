use tokio::signal::unix::{signal, Signal, SignalKind};

pub fn sighup_stream() -> std::io::Result<Signal> {
    signal(SignalKind::hangup())
}

pub fn sigterm_stream() -> std::io::Result<Signal> {
    signal(SignalKind::terminate())
}

pub fn sigint_stream() -> std::io::Result<Signal> {
    signal(SignalKind::interrupt())
}
