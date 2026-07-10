use std::sync::Arc;

/// The scheme used to connect to an HTTP server.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Https,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// Builds the URLs at which the File Transfer Service HTTP server exposes files.
#[derive(Debug, Clone)]
pub struct FileTransferUrls {
    authority: Arc<str>,
    protocol: Protocol,
}

impl FileTransferUrls {
    pub fn new(authority: Arc<str>, protocol: Protocol) -> Self {
        Self {
            authority,
            protocol,
        }
    }

    /// Builds the URL for the given path under the file transfer service's `te/v1/files` API.
    pub fn for_path(&self, path: &str) -> String {
        format!(
            "{}://{}/te/v1/files/{path}",
            self.protocol.as_str(),
            self.authority
        )
    }

    pub fn authority(&self) -> Arc<str> {
        self.authority.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_http_urls() {
        let urls = FileTransferUrls::new("127.0.0.1:8000".into(), Protocol::Http);

        assert_eq!(
            urls.for_path("device/config_snapshot/typeA-1234"),
            "http://127.0.0.1:8000/te/v1/files/device/config_snapshot/typeA-1234"
        );
    }

    #[test]
    fn builds_https_urls() {
        let urls = FileTransferUrls::new("127.0.0.1:8000".into(), Protocol::Https);

        assert_eq!(
            urls.for_path("device/log_upload/typeA-1234"),
            "https://127.0.0.1:8000/te/v1/files/device/log_upload/typeA-1234"
        );
    }
}
