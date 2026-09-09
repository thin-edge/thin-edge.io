use crate::mqtt_topics::EntityTopicId;
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

/// Builds the URLs at which the entity store REST API is reached.
#[derive(Debug, Clone)]
pub struct EntityStoreUrls {
    authority: Arc<str>,
    protocol: Protocol,
}

impl EntityStoreUrls {
    pub fn new(authority: Arc<str>, protocol: Protocol) -> Self {
        Self {
            authority,
            protocol,
        }
    }

    /// Builds the URL of the given entity under the entity store's `te/v1/entities` API.
    pub fn for_entity(&self, topic_id: &EntityTopicId) -> String {
        format!(
            "{}://{}/te/v1/entities/{}",
            self.protocol.as_str(),
            self.authority,
            topic_id.as_str()
        )
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

    #[test]
    fn builds_the_url_of_a_service_from_its_full_topic_id() {
        let urls = EntityStoreUrls::new("127.0.0.1:8000".into(), Protocol::Http);
        let topic_id: EntityTopicId = "device/main/service/collectd".parse().unwrap();

        assert_eq!(
            urls.for_entity(&topic_id),
            "http://127.0.0.1:8000/te/v1/entities/device/main/service/collectd"
        );
    }

    #[test]
    fn builds_the_url_of_a_device_with_its_empty_segments() {
        let urls = EntityStoreUrls::new("127.0.0.1:8000".into(), Protocol::Https);
        let topic_id = EntityTopicId::default_child_device("child01").unwrap();

        assert_eq!(
            urls.for_entity(&topic_id),
            "https://127.0.0.1:8000/te/v1/entities/device/child01//"
        );
    }
}
