pub use self::cli::TEdgeMqttCli;
pub use self::error::MqttError;
use rumqttc::Client;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

mod cli;
mod error;
mod publish;
mod subscribe;

const MAX_PACKET_SIZE: usize = 268435455; // 256 MB

fn disconnect_if_interrupted(client: Client) -> Arc<AtomicBool> {
    let interrupter = Arc::new(Mutex::new(client));
    let interrupted = Arc::new(AtomicBool::new(false));
    for signal in signal_hook::consts::TERM_SIGNALS {
        let interrupter = interrupter.clone();
        let interrupted = interrupted.clone();
        unsafe {
            let _ = signal_hook::low_level::register(*signal, move || {
                interrupted.store(true, Ordering::Relaxed);
                let client = interrupter.lock().unwrap();
                let _ = client.disconnect();
            });
        }
    }
    interrupted
}
