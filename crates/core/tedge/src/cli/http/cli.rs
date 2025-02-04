use crate::cli::http::command::HttpAction;
use crate::cli::http::command::HttpCommand;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Error;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use clap::Args;
use reqwest::blocking;
use reqwest::Identity;
use std::fs::File;
use tedge_config::OptionalConfig;
use tedge_config::ProfileName;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeHttpCli {
    /// POST content to thin-edge local HTTP servers
    Post {
        /// Target URI
        uri: String,

        /// Content to send
        #[command(flatten)]
        content: Content,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// PUT content to thin-edge local HTTP servers
    Put {
        /// Target URI
        uri: String,

        /// Content to send
        #[command(flatten)]
        content: Content,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// GET content from thin-edge local HTTP servers
    Get {
        /// Source URI
        uri: String,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,
    },

    /// DELETE resource from thin-edge local HTTP servers
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

impl TryFrom<Content> for blocking::Body {
    type Error = std::io::Error;

    fn try_from(content: Content) -> Result<Self, Self::Error> {
        let body: blocking::Body = if let Some(data) = content.arg2 {
            data.into()
        } else if let Some(data) = content.data {
            data.into()
        } else if let Some(file) = content.file {
            File::open(file)?.into()
        } else {
            "".into()
        };

        Ok(body)
    }
}

impl BuildCommand for TEdgeHttpCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.load_config()?;
        let uri = self.uri();

        let (protocol, host, port) = if uri.starts_with("/c8y") {
            let c8y_config = config.c8y.try_get(self.c8y_profile())?;
            let client = &c8y_config.proxy.client;
            let protocol = https_if_some(&c8y_config.proxy.cert_path);
            (protocol, client.host.clone(), client.port)
        } else if uri.starts_with("/tedge") {
            let client = &config.http.client;
            let protocol = https_if_some(&config.http.cert_path);
            (protocol, client.host.clone(), client.port)
        } else {
            return Err(anyhow!("Not a local HTTP uri: {uri}").into());
        };

        let url = format!("{protocol}://{host}:{port}{uri}");
        let identity = config.http.client.auth.identity()?;
        let client = http_client(config.cloud_root_certs(), identity.as_ref())?;

        let action = match self {
            TEdgeHttpCli::Post { content, .. } => HttpAction::Post(content),
            TEdgeHttpCli::Put { content, .. } => HttpAction::Put(content),
            TEdgeHttpCli::Get { .. } => HttpAction::Get,
            TEdgeHttpCli::Delete { .. } => HttpAction::Delete,
        };

        Ok(HttpCommand {
            client,
            url,
            action,
        }
        .into_boxed())
    }
}

impl TEdgeHttpCli {
    fn uri(&self) -> &str {
        match self {
            TEdgeHttpCli::Post { uri, .. }
            | TEdgeHttpCli::Put { uri, .. }
            | TEdgeHttpCli::Get { uri, .. }
            | TEdgeHttpCli::Delete { uri, .. } => uri.as_ref(),
        }
    }

    fn c8y_profile(&self) -> Option<&ProfileName> {
        match self {
            TEdgeHttpCli::Post { profile, .. }
            | TEdgeHttpCli::Put { profile, .. }
            | TEdgeHttpCli::Get { profile, .. }
            | TEdgeHttpCli::Delete { profile, .. } => profile.as_ref(),
        }
    }
}

fn https_if_some<T>(cert_path: &OptionalConfig<T>) -> &'static str {
    cert_path.or_none().map_or("http", |_| "https")
}

fn http_client(
    root_certs: CloudRootCerts,
    identity: Option<&Identity>,
) -> Result<blocking::Client, Error> {
    let builder = root_certs.blocking_client_builder();
    let builder = if let Some(identity) = identity {
        builder.identity(identity.clone())
    } else {
        builder
    };
    Ok(builder.build()?)
}
