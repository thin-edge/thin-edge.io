pub use self::cli::TEdgeMqttCli;
pub use self::error::MqttError;
use rumqttc::Client;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

mod cli;
mod error;
mod publish;
mod subscribe;

const MAX_PACKET_SIZE: usize = 268435455; // 256 MB

fn disconnect_if_interrupted(client: Client, timeout: Option<Duration>) -> Arc<AtomicBool> {
    let interrupted = Arc::new(AtomicBool::new(false));
    for signal in signal_hook::consts::TERM_SIGNALS {
        let client = client.clone();
        let interrupted = interrupted.clone();
        unsafe {
            let _ = signal_hook::low_level::register(*signal, move || {
                eprintln!("INFO: Interrupted");
                interrupted.store(true, Ordering::Relaxed);
                let _ = client.disconnect();
            });
        }
    }

    if let Some(timeout) = timeout {
        let timeout_elapsed = interrupted.clone();
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            eprintln!("INFO: Timeout");
            timeout_elapsed.store(true, Ordering::Relaxed);
            let _ = client.disconnect();
        });
    }

    interrupted
}
