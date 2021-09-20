use crate::error::DownloadError;
use c8y_smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_client::{Client, MqttClient, Topic};
use reqwest::{self, Url};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tedge_utils;

struct Downloader;

impl Downloader {}

pub trait Download {
    fn download(&self);
}

pub async fn download(
    url: &str,
    target_dir_path: impl AsRef<Path>,
    target_file_path: impl AsRef<Path>,
) -> Result<(), DownloadError> {
    // TODO: Validate the url belongs to the tenant and we can use jwt token such that we don't leak credentials
    let parsed_url = UrlType::try_new(url)?;

    let response = match parsed_url {
        UrlType::C8y(url) => {
            let mqtt_config = mqtt_client::Config::new("127.0.0.1", 1883);
            let mqtt_client = mqtt_client::Client::connect("downloader", &mqtt_config).await?;
            let token = get_jwt_token(&mqtt_client).await?;

            let client = reqwest::Client::new();
            let response = match client
                .get(url)
                .bearer_auth(token.token())
                .send()
                .await?
                .error_for_status()
            {
                Ok(response) => response,
                Err(err) => {
                    return Err(dbg!(err.into()));
                }
            };
            response
        }

        UrlType::NonC8y(url) => {
            let client = reqwest::Client::new();
            let response = match client.get(url).send().await?.error_for_status() {
                Ok(response) => response,
                Err(err) => {
                    return Err(dbg!(err.into()));
                }
            };
            response
        }

        UrlType::Unsupported(url) => return Err(DownloadError::UnsupportedScheme(url)),
    };

    let content = response.text().await?;

    let temp = PathBuf::new().join("/tmp").join("dl.tmp");
    let target_path = PathBuf::new().join(target_dir_path).join(target_file_path);

    // Cleanup after `disc full` will happen inside atomic write function.
    let () = tedge_utils::fs::atomically_write_file_async(
        temp.as_path(),
        target_path.as_path(),
        content.as_bytes(),
    )
    .await?;

    Ok(())
}

#[derive(Debug)]
enum UrlType {
    C8y(Url),
    NonC8y(Url),
    Unsupported(String),
}

impl UrlType {
    fn try_new(url: &str) -> Result<UrlType, DownloadError> {
        let parsed_url = reqwest::Url::parse(url)?;
        match parsed_url.scheme() {
            "https" | "http" => {}
            _scheme => return Ok(Self::Unsupported(url.to_owned())),
        }
        // TODO: validate if c8y url from config
        Ok(UrlType::C8y(parsed_url))
    }
}

fn confirm_is_my_tenant_from_url(url: &reqwest::Url) -> UrlType {
    // TODO: implement this check
    UrlType::C8y(url.to_owned())
}

async fn get_jwt_token(client: &Client) -> Result<SmartRestJwtResponse, DownloadError> {
    let mut subscriber = client.subscribe(Topic::new("c8y/s/dat")?.filter()).await?;

    let () = client
        .publish(mqtt_client::Message::new(
            &Topic::new("c8y/s/uat")?,
            "".to_string(),
        ))
        .await?;

    let token_smartrest =
        match tokio::time::timeout(Duration::from_secs(10), subscriber.next()).await {
            Ok(Some(msg)) => msg.payload_str()?.to_string(),
            Ok(None) => return Err(DownloadError::InvalidMqttMessage),
            Err(err) => return Err(DownloadError::FromElapsed(err)),
        };

    Ok(SmartRestJwtResponse::try_new(&token_smartrest)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download() {
        let url = "https://lukas-makr11st.latest.stage.c8y.io/inventory/binaries/111200";
        // let url = "https://file-examples-com.github.io/uploads/2017/02/file_example_CSV_5000.csv";;
        let target_dir_path = "/home/makrist/thin-edge.io";
        let target_file_path = "dl.final";

        let res = download(url, target_dir_path, target_file_path).await;
        dbg!(res);
    }
}
