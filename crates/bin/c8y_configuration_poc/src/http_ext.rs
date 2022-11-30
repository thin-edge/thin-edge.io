use tedge_actors::{Recipient, RuntimeHandle};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpConfig {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequest {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {}

/// Create a new HTTP connection managed behind the scene by an actor
///
/// This connection is private,
/// i.e only the callee of `new_private_connection()` will be able to interact with it.
///
/// ```
///       client                    http_con
///             --------------------->|||| ============> http://host
///         ||||<---------------------
/// ```
pub fn new_private_connection(
    runtime: &mut RuntimeHandle,
    config: HttpConfig,
    client: Recipient<HttpResponse>,
) -> Recipient<HttpRequest> {
    todo!()
}
