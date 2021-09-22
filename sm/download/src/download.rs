use crate::error::DownloadError;
use async_trait::async_trait;
use backoff::{backoff::Backoff, future::retry, ExponentialBackoff};
use c8y_smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_client::{Client, MqttClient, Topic};
use reqwest::{self, Url};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tedge_utils;

#[async_trait]
pub trait Downloader {
    async fn download(&self);

    async fn cleanup_downloaded(&self);
}

pub async fn download(
    url: &UrlType,
    target_dir_path: impl AsRef<Path>,
    target_file_name: impl AsRef<Path>,
) -> Result<PathBuf, DownloadError> {
    // TODO: Validate the url belongs to the tenant and we can use jwt token such that we don't leak credentials

    // Default retry is an exponential retry with a limit of 15 minutes total
    let response = retry(ExponentialBackoff::default(), || async {
        Ok(url.clone().get_from_url().await?)
    })
    .await?;

    dbg!(&response);
    let content = response.bytes().await?;

    let temp_path = PathBuf::new().join(&target_dir_path).join("dl.tmp");
    let target_path = PathBuf::new().join(target_dir_path).join(target_file_name);

    dbg!(&target_path);
    // Cleanup after `disc full` will happen inside atomic write function.
    // TODO: Add cleanup on file exists
    let () = tedge_utils::fs::atomically_write_file_async(
        temp_path.as_path(),
        target_path.as_path(),
        content.as_ref(),
    )
    .await?;

    dbg!();
    Ok(target_path)
}

#[derive(Debug, Clone, PartialEq)]
pub enum UrlType {
    C8y(Url),
    NonC8y(Url),
    Unsupported(String),
}

impl UrlType {
    pub fn try_new(url: &str) -> Result<UrlType, DownloadError> {
        let parsed_url = reqwest::Url::parse(url)?;
        match parsed_url.scheme() {
            // TODO: validate if c8y url from config
            "https" => Ok(UrlType::C8y(parsed_url)),
            "http" => Ok(UrlType::NonC8y(parsed_url)),
            _scheme => Ok(Self::Unsupported(url.to_owned())),
        }
    }

    async fn get_from_url(self) -> Result<reqwest::Response, DownloadError> {
        match self {
            UrlType::C8y(url) => {
                let mqtt_config = mqtt_client::Config::new("127.0.0.1", 1883);
                let mqtt_client = mqtt_client::Client::connect("downloader", &mqtt_config).await?;
                let token = get_jwt_token(&mqtt_client).await?;

                let client = reqwest::Client::new();
                match client
                    .get(url)
                    .bearer_auth(token.token())
                    .send()
                    .await?
                    .error_for_status()
                {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        return Err(dbg!(err.into()));
                    }
                }
            }

            UrlType::NonC8y(url) => {
                let client = reqwest::Client::new();
                match client.get(url).send().await?.error_for_status() {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        return Err(dbg!(err.into()));
                    }
                }
            }

            UrlType::Unsupported(url) => Err(DownloadError::UnsupportedScheme(url)),
        }
    }
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
        // * Mock!!!
        let url = "https://lukas-makr11st.latest.stage.c8y.io/inventory/binaries/111200";
        let target_dir_path = "/home/makrist/thin-edge.io";
        let target_file_path = "dl.final";

        let res = download(url, target_dir_path, target_file_path).await;
        dbg!(res);
    }
}
