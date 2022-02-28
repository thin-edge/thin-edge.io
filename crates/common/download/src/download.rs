use crate::error::DownloadError;
use backoff::{future::retry, ExponentialBackoff};
#[cfg(target_os = "linux")]
use nix::fcntl::{fallocate, FallocateFlags};
use nix::sys::statvfs;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Write,
    os::unix::prelude::AsRawFd,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DownloadInfo {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

impl From<&str> for DownloadInfo {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl DownloadInfo {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            auth: None,
        }
    }

    pub fn with_auth(self, auth: Auth) -> Self {
        Self {
            auth: Some(auth),
            ..self
        }
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub enum Auth {
    Bearer(String),
}

impl Auth {
    pub fn new_bearer(token: &str) -> Self {
        Self::Bearer(token.into())
    }
}

#[derive(Debug)]
pub struct Downloader {
    target_filename: PathBuf,
}

impl Downloader {
    pub fn new(name: &str, version: &Option<String>, target_dir_path: impl AsRef<Path>) -> Self {
        let mut filename = name.to_string();
        if let Some(version) = version {
            filename.push('_');
            filename.push_str(version.as_str());
        }

        let target_filename = PathBuf::new().join(target_dir_path).join(filename);

        Self { target_filename }
    }

    pub async fn download(&self, url: &DownloadInfo) -> Result<(), DownloadError> {
        // Default retry is an exponential retry with a limit of 15 minutes total.
        // Let's set some more reasonable retry policy so we don't block the downloads for too long.

        let backoff = ExponentialBackoff {
            initial_interval: Duration::from_secs(30),
            max_elapsed_time: Some(Duration::from_secs(300)),
            ..Default::default()
        };

        let mut response = retry(backoff, || async {
            let client = if let Some(Auth::Bearer(token)) = &url.auth {
                reqwest::Client::new().get(url.url()).bearer_auth(token)
            } else {
                reqwest::Client::new().get(url.url())
            };

            match client
                .send()
                .await
                .map_err(|err| {
                    if err.is_connect() || err.is_builder() {
                        backoff::Error::Permanent(err)
                    } else {
                        log::warn!("Failed to Download. {:?}\nRetrying.", &err);
                        backoff::Error::Transient(err)
                    }
                })?
                .error_for_status()
            {
                Ok(response) => Ok(response),

                Err(err) => match err.status() {
                    Some(status_error) if status_error.is_client_error() => {
                        Err(backoff::Error::Permanent(err))
                    }
                    _ => Err(backoff::Error::Transient(err)),
                },
            }
        })
        .await?;

        let file_len = response.content_length().unwrap_or(0);
        let mut file =
            create_file_and_try_pre_allocate_space(self.target_filename.as_path(), file_len)?;

        while let Some(chunk) = response.chunk().await? {
            if let Err(err) = file.write_all(&chunk) {
                drop(file);
                std::fs::remove_file(self.target_filename.as_path())?;
                return Err(DownloadError::FromIo {
                    reason: format!("Failed to download the file with an error {}", err),
                });
            }
        }

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

fn create_file_and_try_pre_allocate_space(
    file_path: &Path,
    file_len: u64,
) -> Result<File, DownloadError> {
    let file = File::create(file_path)?;
    if file_len > 0 {
        if let Some(root) = file_path.parent() {
            let tmpstats = statvfs::statvfs(root)?;
            // Reserve 5% of total disk space
            let five_percent_disk_space =
                (tmpstats.blocks() as u64 * tmpstats.block_size() as u64) * 5 / 100;
            let usable_disk_space = tmpstats.blocks_free() as u64 * tmpstats.block_size() as u64
                - five_percent_disk_space;

            if file_len >= usable_disk_space {
                return Err(DownloadError::InsufficientSpace);
            }
            // Reserve diskspace
            #[cfg(target_os = "linux")]
            let _ = fallocate(
                file.as_raw_fd(),
                FallocateFlags::empty(),
                0,
                file_len as nix::libc::off_t,
            );
        }
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use crate::DownloadError;

    use super::*;
    use anyhow::bail;
    use mockito::mock;
    use nix::sys::statvfs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::{NamedTempFile, TempDir};
    use test_case::test_case;

    #[test]
    fn construct_downloader_filename() {
        let name = "test_download";
        let version = Some("test1".to_string());
        let target_dir_path = PathBuf::from("/tmp");

        let downloader = Downloader::new(name, &version, &target_dir_path);

        let expected_path = Path::new("/tmp/test_download_test1");
        assert_eq!(downloader.filename(), expected_path);
    }

    #[tokio::test]
    async fn downloader_download_content_no_auth() -> anyhow::Result<()> {
        let _mock1 = mock("GET", "/some_file.txt")
            .with_status(200)
            .with_body(b"hello")
            .create();

        let name = "test_download";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&name, &version, target_dir_path.path());
        let () = downloader.download(&url).await?;

        let log_content = std::fs::read(downloader.filename())?;

        assert_eq!("hello".as_bytes(), log_content);

        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn downloader_download_with_content_length_larger_than_usable_disk_space(
    ) -> anyhow::Result<()> {
        let tmpstats = statvfs::statvfs("/tmp")?;
        let usable_disk_space = tmpstats.blocks_free() as u64 * tmpstats.block_size() as u64;
        let _mock1 = mock("GET", "/some_file.txt")
            .with_header("content-length", &(usable_disk_space.to_string()))
            .create();

        let name = "test_download_with_length";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Err(DownloadError::InsufficientSpace) => return Ok(()),
            _ => bail!("failed"),
        }
    }

    #[tokio::test]
    async fn downloader_download_with_reasonable_content_length() -> anyhow::Result<()> {
        let file = create_file_with_size(10 * 1024 * 1024)?;
        let file_path = file.into_temp_path();

        let _mock1 = mock("GET", "/some_file.txt")
            .with_body_from_file(&file_path)
            .create();

        let name = "test_download_with_length";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&name, &version, target_dir_path.path());

        match downloader.download(&url).await {
            Ok(()) => {
                let log_content = std::fs::read(downloader.filename())?;
                let expected_content = std::fs::read(file_path)?;
                assert_eq!(log_content, expected_content);
                return Ok(());
            }
            _ => bail!("failed"),
        }
    }

    #[tokio::test]
    async fn downloader_download_verify_file_content() -> anyhow::Result<()> {
        let file = create_file_with_size(10)?;

        let _mock1 = mock("GET", "/some_file.txt")
            .with_body_from_file(file.into_temp_path())
            .create();

        let name = "test_download_with_length";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&name, &version, target_dir_path.path());
        downloader.download(&url).await?;

        let log_content = std::fs::read(downloader.filename())?;

        assert_eq!("Some data!".as_bytes(), log_content);

        Ok(())
    }

    #[tokio::test]
    async fn downloader_download_without_content_length() -> anyhow::Result<()> {
        let _mock1 = mock("GET", "/some_file.txt").create();

        let name = "test_download_without_length";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Ok(()) => {
                assert_eq!("".as_bytes(), std::fs::read(downloader.filename())?);
                return Ok(());
            }
            _ => {
                bail!("failed")
            }
        }
    }

    // Parameters:
    //
    // status code
    //
    // bearer token boolean
    //
    // maybe url
    //
    // expected std error
    //
    // description
    #[test_case(
        200,
        false,
        Some("not_a_url"),
        "builder error: relative URL without a base"
        ; "builder error"
    )]
    #[test_case(
        200,
        true,
        Some("not_a_url"),
        "builder error: relative URL without a base"
        ; "builder error with auth"
    )]
    #[test_case(
        200,
        false,
        Some("http://not_a_url"),
        "dns error: failed to lookup address information"
        ; "dns error"
    )]
    #[test_case(
        200,
        true,
        Some("http://not_a_url"),
        "dns error: failed to lookup address information"
        ; "dns error with auth"
    )]
    #[test_case(
        404,
        false,
        None,
        "404 Not Found"
        ; "client error"
    )]
    #[test_case(
        404,
        true,
        None,
        "404 Not Found"
        ; "client error with auth"
    )]
    #[tokio::test]
    async fn downloader_download_processing_error(
        status_code: usize,
        with_token: bool,
        url: Option<&str>,
        expected_err: &str,
    ) -> anyhow::Result<()> {
        let name = "test_download";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        // bearer/no bearer setup
        let _mock1 = {
            if with_token {
                mock("GET", "/some_file.txt")
                    .match_header("authorization", "Bearer token")
                    .with_status(status_code)
                    .create()
            } else {
                mock("GET", "/some_file.txt")
                    .with_status(status_code)
                    .create()
            }
        };

        // url/no url setup
        let url = {
            if let Some(url) = url {
                DownloadInfo::new(&url)
            } else {
                let mut target_url = mockito::server_url();
                target_url.push_str("/some_file.txt");
                DownloadInfo::new(&target_url)
            }
        };

        // applying token if `with_token` = true
        let url = {
            if with_token {
                url.with_auth(Auth::Bearer(String::from("token")))
            } else {
                url
            }
        };

        let downloader = Downloader::new(&name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Ok(_success) => anyhow::bail!("Expected client error."),
            Err(err) => {
                assert!(err.to_string().contains(expected_err));
            }
        };
        Ok(())
    }

    fn create_file_with_size(size: usize) -> Result<NamedTempFile, anyhow::Error> {
        let mut file = NamedTempFile::new()?;
        let data: String = "Some data!".into();
        let loops = size / data.len();
        let mut buffer = String::with_capacity(size);
        for _ in 0..loops {
            buffer.push_str("Some data!");
        }

        file.write_all(buffer.as_bytes())?;

        Ok(file)
    }
}
