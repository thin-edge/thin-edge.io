use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::ConfigError;
use c8y_api::http_proxy::C8yEndPoint;
use std::collections::HashMap;
use std::path::PathBuf;
use tedge_config::tedge_toml::ProfileName;

mod c8y;

#[derive(clap::Subcommand, Debug)]
pub enum UploadCmd {
    /// Upload a file to Cumulocity
    ///
    /// The command creates a new event for the device,
    /// attaches the given file content to this new event,
    /// and returns the event ID.
    C8y {
        /// Path to the uploaded file
        #[clap(long)]
        file: PathBuf,

        /// MIME type of the file content
        ///
        /// If not provided, the mime type is determined from the file extension
        /// If no rules apply, application/octet-stream is taken as a default
        #[clap(long, verbatim_doc_comment)]
        #[arg(value_parser = parse_mime_type)]
        mime_type: Option<String>,

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

        /// Cumulocity external id of the device/service on which the file has to be attached.
        ///
        /// If not given, the file is attached to the main device.
        #[clap(long)]
        device_id: Option<String>,
    },
}

fn parse_json(input: &str) -> Result<HashMap<String, serde_json::Value>, anyhow::Error> {
    Ok(serde_json::from_str(input)?)
}

fn parse_mime_type(input: &str) -> Result<String, anyhow::Error> {
    Ok(input.parse::<mime_guess::mime::Mime>()?.to_string())
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
                let c8y_config = config.c8y.try_get(profile.as_deref())?;
                let device_id = match device_id {
                    None => c8y_config.device.id()?.clone(),
                    Some(device_id) => device_id,
                };
                let text = text.unwrap_or_else(|| format!("Uploaded file: {file:?}"));
                let mime_type = mime_type.unwrap_or_else(|| {
                    mime_guess::from_path(&file)
                        .first_or_octet_stream()
                        .to_string()
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("text/plain")]
    #[test_case("image/svg+xml")]
    #[test_case("foo/bar")]
    #[test_case("foo/bar+zoo")]
    fn accept_mime_type(input: &str) {
        assert_eq!(parse_mime_type(input).ok().as_deref(), Some(input))
    }

    #[test_case("text", "(/) was missing")]
    #[test_case("text/svg/xml", "invalid token")]
    fn reject_incorrect_mime_type(input: &str, error: &str) {
        assert!(parse_mime_type(input)
            .err()
            .unwrap()
            .to_string()
            .contains(error))
    }
}
