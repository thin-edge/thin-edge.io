use super::SystemdService;

pub struct MosquittoService;

impl SystemdService for MosquittoService {
    const SERVICE_NAME: &'static str = "mosquitto";
}
