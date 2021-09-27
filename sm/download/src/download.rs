use crate::error::DownloadError;
use backoff::{future::retry, ExponentialBackoff};
use json_sm::DownloadInfo;
use log::error;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug)]
pub struct Downloader {
    target_filename: PathBuf,
    download_target: PathBuf,
}

impl Downloader {
    pub fn new(name: &str, version: &Option<String>, target_dir_path: impl AsRef<Path>) -> Self {
        let mut filename = name.to_string();
        if let Some(version) = version {
            filename.push('_');
            filename.push_str(version.as_str());
        }

        let mut download_target = PathBuf::new().join(&target_dir_path).join(&filename);
        download_target.set_extension("tmp");
        let target_filename = PathBuf::new().join(target_dir_path).join(filename);

        Self {
            target_filename,
            download_target,
        }
    }

    pub async fn download(&self, url: &DownloadInfo) -> Result<(), DownloadError> {
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

        // Cleanup after `disc full` will happen inside atomic write function.
        tedge_utils::fs::atomically_write_file_async(
            self.download_target.as_path(),
            self.target_filename.as_path(),
            content.as_ref(),
        )
        .await?;

        Ok(())
    }

    pub fn filename(&self) -> &Path {
        self.target_filename.as_path()
    }

    pub async fn cleanup(&self) -> Result<(), DownloadError> {
        let _res = tokio::fs::remove_file(&self.target_filename).await;
        Ok(())
    }
}
