use crate::messages::C8YRestError;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResponse;
use crate::messages::C8YRestResult;
use crate::messages::CreateEvent;
use crate::messages::SoftwareListResponse;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use tedge_actors::ClientMessageBox;
use tedge_actors::Service;

/// Handle to the C8YHttpProxy
#[derive(Clone)]
pub struct C8YHttpProxy {
    c8y: ClientMessageBox<C8YRestRequest, C8YRestResult>,
}

impl C8YHttpProxy {
    pub fn new(proxy_builder: &mut impl Service<C8YRestRequest, C8YRestResult>) -> Self {
        let c8y = ClientMessageBox::new(proxy_builder);
        C8YHttpProxy { c8y }
    }

    pub async fn send_event(&mut self, c8y_event: CreateEvent) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = c8y_event.into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn send_software_list_http(
        &mut self,
        c8y_software_list: C8yUpdateSoftwareListResponse,
        device_id: String,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = SoftwareListResponse {
            c8y_software_list,
            device_id,
        }
        .into();

        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Unit(_)) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    }
}
