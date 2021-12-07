/// Initialize a `tracing_subscriber`
///
/// Reports all the log events sent either with the `log` crate or the `tracing` crate.
///
/// If `debug` is `false` then only `error!`, `warn!` and `info!` are reported.
/// If `debug` is `true` then only `debug!` and `trace!` are reported.
pub fn initialise_tracing_subscriber(debug: bool) {
    let log_level = if debug {
        tracing::Level::TRACE
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            "%Y-%m-%dT%H:%M:%S%.3f%:z".into(),
        ))
        .with_max_level(log_level)
        .init();
}
