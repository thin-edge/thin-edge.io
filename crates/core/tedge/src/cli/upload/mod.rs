use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::ConfigError;
use c8y_api::http_proxy::C8yEndPoint;
use std::collections::HashMap;
use std::path::PathBuf;
use tedge_config::ProfileName;

mod c8y;

#[derive(clap::Subcommand, Debug)]
pub enum UploadCmd {
    /// Upload a file to Cumulocity
    ///
    /// The command create a new event for the device,
    /// attach the given file content to this new event,
    /// and return the event ID.
    C8y {
        /// Path to the uploaded file
        #[clap(long)]
        file: PathBuf,

        /// MIME type of the file content
        #[clap(long, default_value = "application/octet-stream")]
        mime_type: String,

        /// Type of the event
        #[clap(long = "type", default_value = "tedge_UploadedFile")]
        event_type: String,

        /// Text description of the event. Defaults to "Uploaded file: <FILE>"
        #[clap(long)]
        text: Option<String>,

        /// JSON fragment attached to the event
        #[clap(long, default_value = "{}")]
        #[arg(value_parser = parse_json)]
        json: HashMap<String, serde_json::Value>,

        /// Optional c8y cloud profile
        #[clap(long)]
        profile: Option<ProfileName>,

        /// Cumulocity external id of the device child on which the file has to be attached.
        ///
        /// If not given, the file is attached to the main device.
        #[clap(long)]
        device_id: Option<String>,
    },
}

fn parse_json(input: &str) -> Result<HashMap<String, serde_json::Value>, anyhow::Error> {
    Ok(serde_json::from_str(input)?)
}

impl BuildCommand for UploadCmd {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.load_config()?;

        let cmd = match self {
            UploadCmd::C8y {
                event_type,
                text,
                json,
                file,
                mime_type,
                profile,
                device_id,
            } => {
                let identity = config.http.client.auth.identity()?;
                let cloud_root_certs = config.cloud_root_certs();
                let c8y = C8yEndPoint::local_proxy(&config, profile.as_deref())?;
                let device_id = match device_id {
                    None => config.device.id()?.clone(),
                    Some(device_id) => device_id,
                };
                let text = text.unwrap_or_else(|| format!("Uploaded file: {file:?}"));
                c8y::C8yUpload {
                    identity,
                    cloud_root_certs,
                    device_id,
                    c8y,
                    event_type,
                    text,
                    json,
                    file,
                    mime_type,
                }
            }
        };
        Ok(cmd.into_boxed())
    }
}
