use super::SystemdService;

pub struct TedgeMapperService;

impl SystemdService for TedgeMapperService {
    const SERVICE_NAME: &'static str = "tedge-mapper";
}
