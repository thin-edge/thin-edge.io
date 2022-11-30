use async_trait::async_trait;
use tedge_actors::{
    new_mailbox, Actor, ChannelError, Mailbox, Recipient, RuntimeError, RuntimeHandle,
};

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
pub async fn new_private_connection(
    runtime: &mut RuntimeHandle,
    config: HttpConfig,
    client: Recipient<HttpResponse>,
) -> Result<Recipient<HttpRequest>, RuntimeError> {
    let (mailbox, address) = new_mailbox(10);

    let actor = PrivateHttpActor::new(config);
    runtime.run(actor, mailbox, client).await?;

    Ok(address.as_recipient())
}

struct PrivateHttpActor {
    // Some HTTP connection to a remote server
}

impl PrivateHttpActor {
    fn new(_config: HttpConfig) -> Self {
        PrivateHttpActor {}
    }
}

#[async_trait]
impl Actor for PrivateHttpActor {
    type Input = HttpRequest;
    type Output = HttpResponse;
    type Mailbox = Mailbox<HttpRequest>;
    type Peers = Recipient<HttpResponse>;

    async fn run(
        self,
        mut requests: Self::Mailbox,
        mut client: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some(_request) = requests.next().await {
            // Forward the request to the http server
            // Await for a response
            let response = HttpResponse {};

            // Send the response back to the client
            client.send(response).await?
        }

        Ok(())
    }
}
