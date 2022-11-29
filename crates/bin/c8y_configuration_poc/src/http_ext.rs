use tedge_actors::{Recipient, RuntimeHandle};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpConfig {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequest {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {}

/// Create a new HTTP connection managed behind the scene by an actor
pub fn new_connection(
    runtime: &mut RuntimeHandle,
    config: HttpConfig,
    peer: Recipient<HttpResponse>,
) -> Recipient<HttpRequest> {
    todo!()
}
