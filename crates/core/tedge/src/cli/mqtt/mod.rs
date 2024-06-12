pub use self::cli::TEdgeMqttCli;
pub use self::error::MqttError;
use rumqttc::Client;

mod cli;
mod error;
mod publish;
mod subscribe;

const MAX_PACKET_SIZE: usize = 10 * 1024 * 1024;

fn disconnect_if_interrupted(client: Client) {
    use std::sync::Arc;
    use std::sync::Mutex;
    let interrupter = Arc::new(Mutex::new(client));
    for signal in signal_hook::consts::TERM_SIGNALS {
        let interrupter = interrupter.clone();
        unsafe {
            let _ = signal_hook::low_level::register(*signal, move || {
                let mut client = interrupter.lock().unwrap();
                let _ = client.disconnect();
            });
        }
    }
}
