use super::Service;

pub struct MosquittoService;

impl Service for MosquittoService {
    const SERVICE_NAME: &'static str = "mosquitto";
}
