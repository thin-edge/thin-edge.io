use crate::cli::http::cli::Content;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Error;
use hyper::http::HeaderValue;
use mqtt_channel::StreamExt;
use reqwest::header::HeaderMap;
use reqwest::Client;
use reqwest::RequestBuilder;
use tokio::io::AsyncWriteExt;

pub struct HttpCommand {
    /// HTTP client
    pub client: Client,

    /// Target url
    pub url: String,

    /// Action
    pub action: HttpAction,
}

pub enum HttpAction {
    Post {
        content: Content,
        content_type: Option<String>,
        accept_type: Option<String>,
    },
    Put {
        content: Content,
        content_type: Option<String>,
        accept_type: Option<String>,
    },
    Patch {
        content: Content,
        content_type: Option<String>,
        accept_type: Option<String>,
    },
    Get {
        accept_type: Option<String>,
    },
    Delete,
}

#[async_trait::async_trait]
impl Command for HttpCommand {
    fn description(&self) -> String {
        let verb = match self.action {
            HttpAction::Post { .. } => "POST",
            HttpAction::Put { .. } => "PUT",
            HttpAction::Get { .. } => "GET",
            HttpAction::Patch { .. } => "PATCH",
            HttpAction::Delete => "DELETE",
        };
        format!("{verb} {}", self.url)
    }

    async fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        let request = self.request().await?;
        HttpCommand::send(request).await?;
        Ok(())
    }
}

impl HttpCommand {
    async fn request(&self) -> Result<RequestBuilder, Error> {
        let client = &self.client;
        let url = &self.url;
        let headers = self.action.headers();
        let request = match &self.action {
            HttpAction::Post { content, .. } => client
                .post(url)
                .headers(headers)
                .body(content.clone().into_body().await?),
            HttpAction::Put { content, .. } => client
                .put(url)
                .headers(headers)
                .body(content.clone().into_body().await?),
            HttpAction::Patch { content, .. } => client
                .patch(url)
                .headers(headers)
                .body(content.clone().into_body().await?),
            HttpAction::Get { .. } => client.get(url).headers(headers),
            HttpAction::Delete => client.delete(url).headers(headers),
        };

        Ok(request)
    }

    async fn send(request: RequestBuilder) -> Result<(), Error> {
        let http_result = request.send().await?;
        let status = http_result.status();
        if status.is_success() {
            let mut body = http_result.bytes_stream();
            let mut stdout = tokio::io::stdout();
            while let Some(bytes) = body.next().await {
                stdout.write_all(&bytes?).await?;
            }
            Ok(())
        } else {
            let kind = if status.is_client_error() {
                "HTTP client error"
            } else if status.is_server_error() {
                "HTTP server error"
            } else {
                "HTTP error"
            };
            let error = format!(
                "{kind}: {} {}\n{}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                http_result.text().await.unwrap_or("".to_string())
            );
            Err(anyhow!(error))?
        }
    }
}

impl HttpAction {
    pub fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        if let Some(content_length) = self.content_length() {
            headers.insert("Content-Length", content_length);
        }
        if let Some(content_type) = self.content_type() {
            headers.insert("Content-Type", content_type);
        }
        if let Some(accept_type) = self.accept_type() {
            headers.insert("Accept", accept_type);
        }

        headers
    }

    pub fn content_type(&self) -> Option<HeaderValue> {
        match self {
            HttpAction::Post {
                content,
                content_type,
                ..
            }
            | HttpAction::Put {
                content,
                content_type,
                ..
            }
            | HttpAction::Patch {
                content,
                content_type,
                ..
            } => content_type
                .as_ref()
                .cloned()
                .or(content.mime_type())
                .or(Some("application/json".to_string()))
                .and_then(|s| HeaderValue::from_str(&s).ok()),

            _ => None,
        }
    }

    pub fn accept_type(&self) -> Option<HeaderValue> {
        match self {
            HttpAction::Post { accept_type, .. }
            | HttpAction::Put { accept_type, .. }
            | HttpAction::Patch { accept_type, .. }
            | HttpAction::Get { accept_type } => accept_type
                .as_ref()
                .and_then(|s| HeaderValue::from_str(s).ok()),

            _ => None,
        }
    }

    pub fn content_length(&self) -> Option<HeaderValue> {
        match self {
            HttpAction::Post { content, .. }
            | HttpAction::Put { content, .. }
            | HttpAction::Patch { content, .. } => content
                .length()
                .map(|length| length.to_string())
                .and_then(|s| HeaderValue::from_str(&s).ok()),

            _ => None,
        }
    }
}
