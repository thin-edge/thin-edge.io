use crate::cli::http::cli::Content;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use reqwest::blocking;

pub struct HttpCommand {
    /// HTTP client
    pub client: blocking::Client,

    /// Target url
    pub url: String,

    /// Action
    pub action: HttpAction,
}

pub enum HttpAction {
    Post(Content),
    Put(Content),
    Get,
    Delete,
}

impl Command for HttpCommand {
    fn description(&self) -> String {
        let verb = match self.action {
            HttpAction::Post(_) => "POST",
            HttpAction::Put(_) => "PUT",
            HttpAction::Get => "GET",
            HttpAction::Delete => "DELETE",
        };
        format!("{verb} {}", self.url)
    }

    fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        let request = self.request()?;
        HttpCommand::send(request)?;
        Ok(())
    }
}

impl HttpCommand {
    fn request(&self) -> Result<blocking::RequestBuilder, Error> {
        let client = &self.client;
        let url = &self.url;
        let request = match &self.action {
            HttpAction::Post(content) => client
                .post(url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .body(blocking::Body::try_from(content.clone())?),
            HttpAction::Put(content) => client
                .put(url)
                .header("Content-Type", "application/json")
                .body(blocking::Body::try_from(content.clone())?),
            HttpAction::Get => client.get(url).header("Accept", "application/json"),
            HttpAction::Delete => client.delete(url),
        };

        Ok(request)
    }

    fn send(request: blocking::RequestBuilder) -> Result<(), Error> {
        let http_result = request.send()?;
        let http_response = http_result.error_for_status()?;
        let bytes = http_response.bytes()?.to_vec();
        let content = String::from_utf8(bytes)?;

        println!("{content}");
        Ok(())
    }
}
