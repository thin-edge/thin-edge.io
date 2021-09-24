use crate::error::DownloadError;
use async_trait::async_trait;
use backoff::{future::retry, ExponentialBackoff};
use log::error;
use reqwest;
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
    url: &json_sm::DownloadInfo,
    target_dir_path: impl AsRef<Path>,
    target_file_name: impl AsRef<Path>,
) -> Result<PathBuf, DownloadError> {
    // Default retry is an exponential retry with a limit of 15 minutes total.
    // Let's set some more reasonable retry policy so we don't block the downloads for too long.
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_secs(30),
        max_elapsed_time: Some(Duration::from_secs(300)),
        ..Default::default()
    };
    let response = retry(backoff, || async {
        let client = reqwest::Client::new();
        match &url.auth {
            Some(json_sm::Auth::Bearer(token)) => {
                match client
                    .get(url.url())
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        error!("Request returned an error: {:?}", &err);
                        Err(err.into())
                    }
                }
            }
            None => match client.get(url.url()).send().await?.error_for_status() {
                Ok(response) => Ok(response),
                Err(err) => {
                    error!("Request returned an error: {:?}", &err);
                    Err(err.into())
                }
            },
        }
    })
    .await?;

    let content = response.bytes().await?;

    let temp_path = PathBuf::new().join(&target_dir_path).join("dl.tmp");
    let target_path = PathBuf::new().join(target_dir_path).join(target_file_name);

    // Cleanup after `disc full` will happen inside atomic write function.
    // TODO: Add cleanup on file exists
    let () = tedge_utils::fs::atomically_write_file_async(
        temp_path.as_path(),
        target_path.as_path(),
        content.as_ref(),
    )
    .await?;

    Ok(target_path)
}
