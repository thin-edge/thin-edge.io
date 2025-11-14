use crate::cli::http::command::HttpAction;
use crate::cli::http::command::HttpCommand;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Error;
use camino::Utf8PathBuf;
use certificate::CloudHttpConfig;
use clap::Args;
use reqwest::Body;
use reqwest::Client;
use reqwest::Identity;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::OptionalConfig;
use tedge_config::TEdgeConfig;
use tokio::fs::File;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeHttpCli {
    /// GET content from thin-edge local HTTP servers
    ///
    /// Examples:
    ///   # Download file from the file transfer service
    ///   tedge http get /te/v1/files/target.txt
    ///
    ///   # Download file from Cumulocity's binary api
    ///   tedge http get /c8y/inventory/binaries/104332 > my_file.bin
    #[clap(verbatim_doc_comment)]
    Get {
        /// Source URI
        uri: String,

        /// MIME type of the expected content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        accept_type: Option<String>,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// POST content to thin-edge local HTTP servers
    ///
    /// Examples:
    ///   # Create a new Cumulocity Managed Object via the proxy service
    ///   tedge http post /c8y/inventory/managedObjects '{"name":"test"}' --accept-type application/json
    ///
    ///   # Create a new child device
    ///   tedge http post /te/v1/entities '{
    ///       "@topic-id": "device/a//",
    ///       "@type": "child-device",
    ///       "@parent": "device/main//"
    ///   }'
    #[clap(verbatim_doc_comment)]
    Post {
        /// Target URI
        uri: String,

        /// Content to send
        #[command(flatten)]
        content: Content,

        /// MIME type of the content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        content_type: Option<String>,

        /// MIME type of the expected content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        accept_type: Option<String>,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// PUT content to thin-edge local HTTP servers
    ///
    /// Examples:
    ///   # Upload file to the file transfer service
    ///   tedge http put /te/v1/files/target.txt --file source.txt
    ///
    ///   # Update a Cumulocity Managed Object. Note: Assuming tedge is the owner of the managed object
    ///   tedge http put /c8y/inventory/managedObjects/2343978440 '{"name":"item A"}' --accept-type application/json
    #[clap(verbatim_doc_comment)]
    Put {
        /// Target URI
        uri: String,

        /// Content to send
        #[command(flatten)]
        content: Content,

        /// MIME type of the content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        content_type: Option<String>,

        /// MIME type of the expected content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        accept_type: Option<String>,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// PATCH content to thin-edge local HTTP servers
    ///
    /// Examples:
    ///   # Patch child device twin data
    ///   tedge http patch /te/v1/entities/device/child01 '{"type": "Raspberry Pi 4", "serialNo": "98761234"}'
    #[clap(verbatim_doc_comment)]
    Patch {
        /// Target URI
        uri: String,

        /// Content to send
        #[command(flatten)]
        content: Content,

        /// MIME type of the content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        content_type: Option<String>,

        /// MIME type of the expected content
        #[clap(long)]
        #[arg(value_parser = parse_mime_type)]
        accept_type: Option<String>,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// DELETE resource from thin-edge local HTTP servers
    ///
    /// Examples:
    ///   # Delete a file from the file transfer service
    ///   tedge http delete /te/v1/files/target.txt
    ///
    ///   # Delete a Cumulocity managed object. Note: Assuming tedge is the owner of the managed object
    ///   tedge http delete /c8y/inventory/managedObjects/2343978440
    #[clap(verbatim_doc_comment)]
    Delete {
        /// Source URI
        uri: String,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },
}

#[derive(Args, Clone, Debug)]
#[group(required = true, multiple = false)]
pub struct Content {
    /// Content to send
    #[arg(name = "content")]
    arg2: Option<String>,

    /// Content to send
    #[arg(long)]
    data: Option<String>,

    /// File which content is sent
    #[arg(long)]
    file: Option<Utf8PathBuf>,
}

fn parse_mime_type(input: &str) -> Result<String, Error> {
    Ok(input.parse::<mime_guess::mime::Mime>()?.to_string())
}

impl Content {
    pub async fn into_body(self) -> Result<Body, std::io::Error> {
        let body: Body = if let Some(data) = self.arg2 {
            data.into()
        } else if let Some(data) = self.data {
            data.into()
        } else if let Some(file) = self.file {
            File::open(file).await?.into()
        } else {
            "".into()
        };

        Ok(body)
    }
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeHttpCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let uri = self.uri();

        let (protocol, host, port) = if uri.starts_with("/c8y") {
            let c8y_config = config
                .mapper_config::<C8yMapperSpecificConfig>(&self.c8y_profile())
                .await?;
            let client = &c8y_config.cloud_specific.proxy.client;
            let protocol = https_if_some(&c8y_config.cloud_specific.proxy.cert_path);
            (protocol, client.host.clone(), client.port)
        } else if uri.starts_with("/tedge") || uri.starts_with("/te") {
            let client = &config.http.client;
            let protocol = https_if_some(&config.http.cert_path);
            (protocol, client.host.clone(), client.port)
        } else {
            return Err(anyhow!("Not a local HTTP uri: {uri}").into());
        };

        let url = format!("{protocol}://{host}:{port}{uri}");
        let identity = config.http.client.auth.identity()?;
        let client = http_client(config.cloud_root_certs()?, identity.as_ref())?;
        let action = self.into();

        Ok(HttpCommand {
            client,
            url,
            action,
        }
        .into_boxed())
    }
}

impl From<TEdgeHttpCli> for HttpAction {
    fn from(value: TEdgeHttpCli) -> Self {
        match value {
            TEdgeHttpCli::Post {
                content,
                content_type,
                accept_type,
                ..
            } => HttpAction::Post {
                content,
                content_type,
                accept_type,
            },
            TEdgeHttpCli::Put {
                content,
                content_type,
                accept_type,
                ..
            } => HttpAction::Put {
                content,
                content_type,
                accept_type,
            },
            TEdgeHttpCli::Patch {
                content,
                content_type,
                accept_type,
                ..
            } => HttpAction::Patch {
                content,
                content_type,
                accept_type,
            },
            TEdgeHttpCli::Get { accept_type, .. } => HttpAction::Get { accept_type },
            TEdgeHttpCli::Delete { .. } => HttpAction::Delete,
        }
    }
}

impl TEdgeHttpCli {
    fn uri(&self) -> &str {
        match self {
            TEdgeHttpCli::Post { uri, .. }
            | TEdgeHttpCli::Put { uri, .. }
            | TEdgeHttpCli::Get { uri, .. }
            | TEdgeHttpCli::Patch { uri, .. }
            | TEdgeHttpCli::Delete { uri, .. } => uri.as_ref(),
        }
    }

    fn c8y_profile(&self) -> Option<&ProfileName> {
        match self {
            TEdgeHttpCli::Post { profile, .. }
            | TEdgeHttpCli::Put { profile, .. }
            | TEdgeHttpCli::Get { profile, .. }
            | TEdgeHttpCli::Patch { profile, .. }
            | TEdgeHttpCli::Delete { profile, .. } => profile.as_ref(),
        }
    }
}

fn https_if_some<T>(cert_path: &OptionalConfig<T>) -> &'static str {
    cert_path.or_none().map_or("http", |_| "https")
}

fn http_client(http_config: CloudHttpConfig, identity: Option<&Identity>) -> Result<Client, Error> {
    let builder = http_config.client_builder();
    let builder = if let Some(identity) = identity {
        builder.identity(identity.clone())
    } else {
        builder
    };
    Ok(builder.build()?)
}

impl Content {
    pub fn length(&self) -> Option<usize> {
        if let Some(content) = &self.arg2 {
            Some(content.len())
        } else if let Some(data) = &self.data {
            Some(data.len())
        } else if let Some(file) = &self.file {
            Some(std::fs::metadata(file).ok()?.len().try_into().ok()?)
        } else {
            None
        }
    }

    pub fn mime_type(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        Some(
            mime_guess::from_path(file)
                .first_or_octet_stream()
                .to_string(),
        )
    }
}
