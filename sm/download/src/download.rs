use crate::error::DownloadError;
use backoff::{future::retry, ExponentialBackoff};
use json_sm::DownloadInfo;
use nix::{
    fcntl::{fallocate, FallocateFlags},
    sys::statvfs,
};
use std::{
    fs::File,
    io::Write,
    os::unix::prelude::AsRawFd,
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
            let client = if let Some(json_sm::Auth::Bearer(token)) = &url.auth {
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
        let mut response = response;
        let mut file = File::create(self.target_filename.as_path())?;

        if let Some(file_len) = response.content_length() {
            dbg!(file_len);
            let tmpstats = statvfs::statvfs("/tmp")?;
            let usable_disk_space = tmpstats.blocks_free() * tmpstats.block_size();
            dbg!(usable_disk_space);
            if file_len >= usable_disk_space {
                return Err(DownloadError::NotEnoughDiskspace);
            }
            // Reserve 5% of total disk space
            let five_percent_disk_space = (tmpstats.blocks() * tmpstats.block_size()) * 5 / 100;

            if five_percent_disk_space > (usable_disk_space - file_len) {
                return Err(DownloadError::NotEnoughDiskspace);
            }
            // Reserve diskspace
            if let Err(err) = fallocate(
                file.as_raw_fd(),
                FallocateFlags::empty(),
                0,
                file_len as i64,
            ) {
                return Err(DownloadError::FromNix(err));
            }
        };
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

#[cfg(test)]
mod tests {
    use crate::DownloadError;

    use super::Downloader;
    use anyhow::bail;
    use json_sm::{Auth, DownloadInfo};
    use mockito::mock;
    use nix::sys::statvfs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
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

    #[tokio::test]
    async fn downloader_download_with_content_length() -> anyhow::Result<()> {
        let tmpstats = statvfs::statvfs("/tmp")?;
        let usable_disk_space = tmpstats.blocks_free() * tmpstats.block_size();
        dbg!("{:?}", usable_disk_space);
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
            Err(DownloadError::NotEnoughDiskspace) => return Ok(()),
            _ => return Err(bail!("failed")),
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
}
