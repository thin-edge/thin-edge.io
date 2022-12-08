use crate::{HttpConfig, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;
use tedge_actors::{Actor, ChannelError, Mailbox, Recipient};

pub(crate) struct HttpActor {
    client: reqwest::Client,
}

impl HttpActor {
    pub(crate) fn new(_config: HttpConfig) -> Result<Self, HttpError> {
        let client = reqwest::Client::builder().build()?;
        Ok(HttpActor { client })
    }
}

#[async_trait]
impl Actor for HttpActor {
    type Input = (usize, HttpRequest);
    type Output = (usize, Result<HttpResponse, HttpError>);
    type Mailbox = Mailbox<Self::Input>;
    type Peers = Recipient<Self::Output>;

    async fn run(
        self,
        mut requests: Self::Mailbox,
        mut client: Self::Peers,
    ) -> Result<(), ChannelError> {
        while let Some((client_id, request)) = requests.next().await {
            // Forward the request to the http server
            let request = request.into();

            // Await for a response
            let response = match self.client.execute(request).await {
                Ok(res) => Ok(res.into()),
                Err(err) => Err(err.into()),
            };

            // Send the response back to the client
            client.send((client_id, response)).await?
        }

        Ok(())
    }
}
