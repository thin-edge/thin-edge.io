mod messages;
mod tasks;
pub use messages::*;

use async_trait::async_trait;
use std::collections::HashMap;
use tedge_actors::{Actor, Recipient, RuntimeError, RuntimeHandler};

pub struct SoftwareModuleManager {
    module_types: HashMap<SoftwareType, Recipient<SMRequest>>,
}

#[async_trait]
impl Actor for SoftwareModuleManager {
    type Config = ();
    type Input = SMManagerRequest;
    type Output = SMManagerResponse;

    fn try_new(config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(SoftwareModuleManager {
            module_types: HashMap::new(),
        })
    }

    async fn start(
        &mut self,
        _runtime: RuntimeHandler,
        _output: Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn react(
        &mut self,
        message: Self::Input,
        runtime: &mut RuntimeHandler,
        output: &mut Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        match message {
            SMManagerRequest::RegisterSoftwareModule(request) => {
                let registration = self.register(request);
                output.send_message(registration.into()).await
            }
            SMManagerRequest::ListSoftwareModules(request) => {}
            SMManagerRequest::UpdateSoftwareModules(request) => {}
            SMManagerRequest::UpdateSoftwareModule(request) => {}
        }
    }
}

impl SoftwareModuleManager {
    fn register(&mut self, request: RegisterSoftwareModule) -> SoftwareModuleRegistration {
        self.module_types
            .insert(request.module_type.clone(), request.actor);
        SoftwareModuleRegistration {
            module_type: request.module_type,
        }
    }
}
