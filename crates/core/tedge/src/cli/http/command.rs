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
    Post {
        content: Content,
        content_type: String,
        accept_type: String,
    },
    Put {
        content: Content,
        content_type: String,
    },
    Get {
        accept_type: String,
    },
    Delete,
}

impl Command for HttpCommand {
    fn description(&self) -> String {
        let verb = match self.action {
            HttpAction::Post { .. } => "POST",
            HttpAction::Put { .. } => "PUT",
            HttpAction::Get { .. } => "GET",
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
            HttpAction::Post {
                content,
                content_type,
                accept_type,
            } => client
                .post(url)
                .header("Accept", accept_type)
                .header("Content-Type", content_type)
                .body(blocking::Body::try_from(content.clone())?),
            HttpAction::Put {
                content,
                content_type,
            } => client
                .put(url)
                .header("Content-Type", content_type)
                .body(blocking::Body::try_from(content.clone())?),
            HttpAction::Get { accept_type } => client.get(url).header("Accept", accept_type),
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
