use crate::error::DownloadError;
use reqwest;
use std::{
    fs::File,
    io::copy,
    path::{Path, PathBuf},
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
    let tmp_dir = tempfile::Builder::new().prefix("example").tempdir()?;
    // let url = "https://c8y.io/file";
    let response = reqwest::get(url).await?;

    let content = response.text().await?;
    let temp = tempfile::NamedTempFile::new_in(&target_dir_path)?;
    let target_path = PathBuf::new().join(target_dir_path).join(target_file_path);
    tedge_utils::fs::atomically_write_file_async(temp.path(), target_path, content.as_bytes())
        .await?;

    Ok(())
}
