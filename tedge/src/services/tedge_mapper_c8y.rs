use super::SystemdService;

pub struct TedgeMapperC8yService;

impl SystemdService for TedgeMapperC8yService {
    const SERVICE_NAME: &'static str = "tedge-mapper-c8y";
}
