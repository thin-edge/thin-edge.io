use super::SystemdService;

pub struct TedgeMapperAzService;

impl SystemdService for TedgeMapperAzService {
    const SERVICE_NAME: &'static str = "tedge-mapper-az";
}
