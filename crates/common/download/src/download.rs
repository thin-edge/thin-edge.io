pub mod partial_response;
use crate::download::partial_response::PartialResponse;
use crate::error::DownloadError;
use crate::error::ErrContext;
use anyhow::anyhow;
use backoff::future::retry_notify;
use backoff::ExponentialBackoff;
use certificate::CloudHttpConfig;
use http::StatusCode;
use log::debug;
use log::info;
use log::warn;
use nix::sys::statvfs;
pub use partial_response::InvalidResponseError;
use reqwest::header::HeaderMap;
use reqwest::Client;
use reqwest::Identity;
use reqwest::Response;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_utils::file::FileError;

#[cfg(target_os = "linux")]
use nix::fcntl::fallocate;
#[cfg(target_os = "linux")]
use nix::fcntl::FallocateFlags;

fn default_backoff() -> ExponentialBackoff {
    // Default retry is an exponential retry with a limit of 15 minutes total.
    // Let's set some more reasonable retry policy so we don't block the downloads for too long.
    ExponentialBackoff {
        initial_interval: Duration::from_secs(15),
        max_elapsed_time: Some(Duration::from_secs(300)),
        randomization_factor: 0.1,
        ..Default::default()
    }
}

/// Describes a request used to retrieve the file.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DownloadInfo {
    pub url: String,
    #[serde(skip)]
    pub headers: HeaderMap,
}

impl From<&str> for DownloadInfo {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl DownloadInfo {
    /// Creates new [`DownloadInfo`] from a URL.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            headers: HeaderMap::new(),
        }
    }

    /// Creates new [`DownloadInfo`] from a URL with authentication.
    pub fn with_headers(self, header_map: HeaderMap) -> Self {
        Self {
            headers: header_map,
            ..self
        }
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.url.trim().is_empty()
    }
}

/// A struct which manages file downloads.
#[derive(Debug)]
pub struct Downloader {
    target_filename: PathBuf,
    backoff: ExponentialBackoff,
    client: Client,
}

impl Downloader {
    /// Creates a new downloader which downloads to a target directory and uses
    /// default permissions.
    pub fn new(
        target_path: PathBuf,
        identity: Option<Identity>,
        cloud_http_config: CloudHttpConfig,
    ) -> Self {
        let mut client_builder = cloud_http_config.client_builder();
        if let Some(identity) = identity {
            client_builder = client_builder.identity(identity);
        }
        let client = client_builder.build().expect("Client builder is valid");
        Self {
            target_filename: target_path,
            backoff: default_backoff(),
            client,
        }
    }

    pub fn set_backoff(&mut self, backoff: ExponentialBackoff) {
        self.backoff = backoff;
    }

    /// Downloads a file using an exponential backoff strategy.
    ///
    /// Partial backoff has a minimal interval of 30s and max elapsed time of
    /// 5min. It applies only when sending a request and either receiving a
    /// 500-599 response status or when request couldn't be made due to some
    /// network-related failure. If a network failure happens when downloading
    /// response body chunks, in some cases it doesn't trigger any errors, but
    /// just grinds down to a halt, e.g. when disconnecting from a network.
    ///
    /// To learn more about the backoff, see documentation of the
    /// [`backoff`](backoff) crate.
    ///
    /// Requests partial ranges if a transient error happened while downloading
    /// and the server response included `Accept-Ranges` header.
    pub async fn download(&self, url: &DownloadInfo) -> Result<(), DownloadError> {
        let tmp_target_path = self.temp_filename().await?;
        let target_file_path = self.target_filename.as_path();

        let temp_dir = self
            .target_filename
            .parent()
            .unwrap_or(&self.target_filename);

        let mut file = tempfile::NamedTempFile::new_in(temp_dir)
            .context("Could not write to temporary file".to_string())?;

        let mut response = self.request_range_from(url, 0).await?;

        let file_len = response.content_length().unwrap_or(0);
        info!(
            "Downloading file from url={url:?}, len={file_len}",
            url = url.url
        );

        if file_len > 0 {
            try_pre_allocate_space(file.as_file(), &tmp_target_path, file_len)?;
            debug!("preallocated space for file {tmp_target_path:?}, len={file_len}");
        }

        if let Err(err) = save_chunks_to_file_at(&mut response, file.as_file_mut(), 0).await {
            match err {
                SaveChunksError::Network(err) => {
                    warn!("Error while downloading response: {err}.\nRetrying...");

                    self.download_continue(url, file.as_file_mut(), response)
                        .await?;
                }
                SaveChunksError::Io(err) => {
                    return Err(DownloadError::FromIo {
                        source: err,
                        context: "Error while saving to file".to_string(),
                    })
                }
            }
        }

        // Move the downloaded file to the final destination
        debug!(
            "Moving downloaded file from {:?} to {:?}",
            &tmp_target_path, &target_file_path
        );

        file.persist(target_file_path)
            .map_err(|p| p.error)
            .context("Could not persist temporary file".to_string())?;

        Ok(())
    }

    /// If interrupted, continues ongoing download.
    ///
    /// If the server supports it, a range request is used to download only the
    /// remaining range of the file. Otherwise, progress is restarted and we
    /// download full range of the file again.
    async fn download_continue(
        &self,
        url: &DownloadInfo,
        file: &mut File,
        mut prev_response: Response,
    ) -> Result<(), DownloadError> {
        let mut last_result = Ok(());
        for _ in 0..4 {
            let request_offset = next_request_offset(&prev_response, file)?;
            let mut response = self.request_range_from(url, request_offset).await?;
            let offset = match partial_response::response_range_start(&response, &prev_response)? {
                PartialResponse::CompleteContent => 0,
                PartialResponse::PartialContent(pos) => pos,
                PartialResponse::ResourceModified => {
                    file.seek(SeekFrom::Start(0))
                        .context("failed to seek in file".to_string())?;
                    continue;
                }
            };

            if offset != 0 {
                info!("Resuming file download at position={offset}");
            } else {
                info!("Could not resume download, restarting");
            }

            match save_chunks_to_file_at(&mut response, file, offset).await {
                Ok(()) => {
                    last_result = Ok(());
                    break;
                }

                Err(SaveChunksError::Network(err)) => {
                    warn!("Error while downloading response: {err}.\nRetrying...");
                    last_result = Err(DownloadError::Request(err));
                }

                Err(SaveChunksError::Io(err)) => {
                    return Err(DownloadError::FromIo {
                        source: err,
                        context: "Error while saving to file".to_string(),
                    })
                }
            };
            prev_response = response;
        }

        last_result
    }

    /// Returns the filename.
    pub fn filename(&self) -> &Path {
        self.target_filename.as_path()
    }

    /// Builds a temporary filename the file will be downloaded into.
    async fn temp_filename(&self) -> Result<PathBuf, DownloadError> {
        if self.target_filename.is_relative() {
            return Err(FileError::InvalidFileName {
                path: self.target_filename.clone(),
                source: anyhow!("Path can't be relative"),
            })?;
        }

        if self.target_filename.exists() {
            // Confirm that the file has write access before any http request attempt
            self.has_write_access()?;
        } else if let Some(file_parent) = self.target_filename.parent() {
            if !file_parent.exists() {
                tokio::fs::create_dir_all(file_parent)
                    .await
                    .context(format!(
                        "error creating parent directories for {file_parent:?}"
                    ))?;
            }
        }

        // Download file to the target directory with a temp name
        let target_file_path = &self.target_filename;
        let file_name = target_file_path
            .file_name()
            .ok_or_else(|| FileError::InvalidFileName {
                path: target_file_path.clone(),
                source: anyhow!("Does not name a valid file"),
            })?
            .to_str()
            .ok_or_else(|| FileError::InvalidFileName {
                path: target_file_path.clone(),
                source: anyhow!("Path is not valid unicode"),
            })?;
        let parent_dir = target_file_path
            .parent()
            .ok_or_else(|| FileError::InvalidFileName {
                path: target_file_path.clone(),
                source: anyhow!("Does not name a valid file"),
            })?;

        let tmp_file_name = format!("{file_name}.tmp");
        Ok(parent_dir.join(tmp_file_name))
    }

    fn has_write_access(&self) -> Result<(), DownloadError> {
        let metadata = if self.target_filename.is_file() {
            let target_filename = &self.target_filename;
            fs::metadata(target_filename)
                .context(format!("error getting metadata of {target_filename:?}"))?
        } else {
            // If the file does not exist before downloading file, check the directory perms
            let parent_dir =
                &self
                    .target_filename
                    .parent()
                    .ok_or_else(|| DownloadError::NoWriteAccess {
                        path: self.target_filename.clone(),
                    })?;
            fs::metadata(parent_dir).context(format!("error getting metadata of {parent_dir:?}"))?
        };

        // Write permission check
        if metadata.permissions().readonly() {
            Err(DownloadError::NoWriteAccess {
                path: self.target_filename.clone(),
            })
        } else {
            Ok(())
        }
    }

    /// Deletes the file if it was downloaded.
    pub async fn cleanup(&self) -> Result<(), DownloadError> {
        let _res = tokio::fs::remove_file(&self.target_filename).await;
        Ok(())
    }

    /// Requests either the entire HTTP resource, or its part, from an offset to the
    /// end.
    ///
    /// If `range_start` is `0`, then a regular GET request is sent. Otherwise, a
    /// request for a range of the resource, starting from `range_start`, until EOF,
    /// is sent.
    ///
    /// We use a half-open range with only a lower bound, because we expect to use
    /// it to download static resources which do not change, and only as a recovery
    /// mechanism in case of network failures.
    pub async fn request_range_from(
        &self,
        url: &DownloadInfo,
        range_start: u64,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let backoff = self.backoff.clone();

        let operation = || async {
            let mut request = self.client.get(url.url());
            request = request.headers(url.headers.clone());

            if range_start != 0 {
                request = request.header("Range", format!("bytes={range_start}-"));
            }

            request
                .send()
                .await
                .and_then(|response| {
                    if response.status() != StatusCode::RANGE_NOT_SATISFIABLE {
                        response.error_for_status()
                    } else {
                        // if range not satisfiable, request should be retried with different range
                        // (or without)
                        Ok(response)
                    }
                })
                .map_err(reqwest_err_to_backoff)
        };

        retry_notify(backoff, operation, |err, dur: Duration| {
            let dur = dur.as_secs();
            warn!("Temporary failure: {err}. Retrying in {dur}s",)
        })
        .await
    }
}

/// Decides whether HTTP request error is retryable.
fn reqwest_err_to_backoff(err: reqwest::Error) -> backoff::Error<reqwest::Error> {
    if err.is_timeout() || err.is_connect() {
        return backoff::Error::transient(err);
    }
    if let Some(status) = err.status() {
        if status.is_server_error() {
            return backoff::Error::transient(err);
        }
    }
    backoff::Error::permanent(err)
}

/// Saves a response body chunks starting from an offset.
async fn save_chunks_to_file_at(
    response: &mut reqwest::Response,
    writer: &mut File,
    offset: u64,
) -> Result<(), SaveChunksError> {
    writer.seek(SeekFrom::Start(offset))?;

    while let Some(bytes) = response.chunk().await? {
        writer.write_all(&bytes)?;
    }
    writer.flush()?;
    let end_pos = writer.stream_position()?;
    writer.set_len(end_pos)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum SaveChunksError {
    #[error("Error reading from network")]
    Network(#[from] reqwest::Error),

    #[error("Unable to write data to the file")]
    Io(#[from] std::io::Error),
}

#[allow(clippy::unnecessary_cast)]
pub fn try_pre_allocate_space(
    file: &File,
    path: &Path,
    file_len: u64,
) -> Result<(), DownloadError> {
    if file_len == 0 {
        return Ok(());
    }

    let tmpstats =
        statvfs::fstatvfs(file).context(format!("Can't stat file descriptor for file {path:?}"))?;

    // Reserve 5% of total disk space
    let five_percent_disk_space =
        (tmpstats.blocks() as i64 * tmpstats.block_size() as i64) * 5 / 100;
    let usable_disk_space =
        tmpstats.blocks_free() as i64 * tmpstats.block_size() as i64 - five_percent_disk_space;

    if file_len >= usable_disk_space.max(0) as u64 {
        return Err(DownloadError::InsufficientSpace);
    }

    // Reserve diskspace
    #[cfg(target_os = "linux")]
    let _ = fallocate(
        file,
        FallocateFlags::empty(),
        0,
        file_len.try_into().expect("file too large to fit in i64"),
    );

    Ok(())
}

fn next_request_offset(prev_response: &Response, file: &mut File) -> Result<u64, DownloadError> {
    use hyper::header;
    use hyper::StatusCode;
    use std::io::Seek;
    let pos = file
        .stream_position()
        .context("failed to get cursor position".to_string())?;
    if prev_response.status() == StatusCode::PARTIAL_CONTENT
        || prev_response
            .headers()
            .get(header::ACCEPT_RANGES)
            .is_some_and(|unit| unit == "bytes")
    {
        Ok(pos)
    } else {
        Ok(0)
    }
}

#[cfg(test)]
mod tests;
