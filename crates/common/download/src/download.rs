mod partial_response;
pub use partial_response::InvalidResponseError;
use tedge_utils::file::FileError;

use crate::error::DownloadError;
use crate::error::ErrContext;
use backoff::future::retry;
use backoff::ExponentialBackoff;
use log::debug;
use log::warn;
use nix::sys::statvfs;
use reqwest::header;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::os::unix::prelude::AsRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_utils::file::move_file;
use tedge_utils::file::PermissionEntry;

#[cfg(target_os = "linux")]
use nix::fcntl::fallocate;
#[cfg(target_os = "linux")]
use nix::fcntl::FallocateFlags;

const BACKOFF_INITIAL_INTERVAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX_ELAPSED: Duration = Duration::from_secs(300);

/// Describes a request used to retrieve the file.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Creates new [`DownloadInfo`] from a URL.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.into(),
            auth: None,
        }
    }

    /// Creates new [`DownloadInfo`] from a URL with authentication.
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

/// Possible authentication schemes
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub enum Auth {
    /// HTTP Bearer authentication
    Bearer(String),
}

impl Auth {
    pub fn new_bearer(token: &str) -> Self {
        Self::Bearer(token.into())
    }
}

/// A struct which manages file downloads.
#[derive(Debug)]
pub struct Downloader {
    target_filename: PathBuf,
    target_permission: PermissionEntry,
}

impl From<PathBuf> for Downloader {
    fn from(path: PathBuf) -> Self {
        Self {
            target_filename: path,
            target_permission: PermissionEntry::default(),
        }
    }
}

impl Downloader {
    /// Creates a new downloader which downloads to a target directory and sets
    /// specified permissions the downloaded file.
    pub fn new(target_path: &Path, target_permission: PermissionEntry) -> Self {
        Self {
            target_filename: target_path.to_path_buf(),
            target_permission,
        }
    }

    /// Creates a new downloader which renames downloaded files as software modules.
    #[deprecated(note = "Use `new` instead")]
    pub fn new_sm(name: &str, version: &Option<String>, target_dir_path: impl AsRef<Path>) -> Self {
        let mut filename = name.to_string();
        if let Some(version) = version {
            filename.push('_');
            filename.push_str(version.as_str());
        }

        let target_filename = PathBuf::new().join(target_dir_path).join(filename);

        target_filename.into()
    }

    /// Downloads a file using an exponential backoff strategy.
    ///
    /// Partial backoff has a minimal interval of 30s and max elapsed time of
    /// 5min. To learn more about the backoff, see documentation of the
    /// [`backoff`](backoff) crate.
    ///
    /// Requests partial ranges if a transient error happened while downloading
    /// and the server response included `Accept-Ranges` header.
    pub async fn download(&self, url: &DownloadInfo) -> Result<(), DownloadError> {
        let tmp_target_path = self.temp_filename().await?;
        let target_file_path = self.target_filename.as_path();

        let mut file: File = File::create(&tmp_target_path)
            .context(format!("Can't create a temporary file {tmp_target_path:?}"))?;

        let mut response = request_range_from(url, 0).await?;

        let file_len = response.content_length().unwrap_or(0);
        debug!("Downloading file, len={file_len}");

        if file_len > 0 {
            try_pre_allocate_space(&mut file, &tmp_target_path, file_len)?;
            debug!("preallocated space for file {tmp_target_path:?}, len={file_len}");
        }

        if let Err(err) = save_chunks_to_file_at(&mut response, &mut file, 0).await {
            match err {
                SaveChunksError::Network(err) => {
                    warn!("Error while downloading response: {err}.\nRetrying...");

                    match response.headers().get(header::ACCEPT_RANGES) {
                        Some(unit) if unit == "bytes" => {
                            self.download_remaining(url, &mut file).await?;
                        }
                        _ => {
                            self.retry(url, &mut file).await?;
                        }
                    }
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
        move_file(
            tmp_target_path,
            target_file_path,
            self.target_permission.clone(),
        )
        .await?;

        Ok(())
    }

    /// Retries the download requesting only the remaining file part.
    ///
    /// If the server does support it, a range request is used to download only
    /// the remaining range of the file. If the range request could not be used,
    /// [`retry`](Downloader::retry) is used instead.
    async fn download_remaining(
        &self,
        url: &DownloadInfo,
        file: &mut File,
    ) -> Result<(), DownloadError> {
        loop {
            let file_pos = file
                .stream_position()
                .context("Can't get file cursor position".to_string())?;

            let mut response = request_range_from(url, file_pos).await?;

            let offset = partial_response::response_range_start(&response)?;

            match save_chunks_to_file_at(&mut response, file, offset).await {
                Ok(()) => break,

                Err(SaveChunksError::Network(err)) => {
                    warn!("Error while downloading response: {err}.\nRetrying...");
                    continue;
                }

                Err(SaveChunksError::Io(err)) => {
                    return Err(DownloadError::FromIo {
                        source: err,
                        context: "Error while saving to file".to_string(),
                    })
                }
            }
        }

        Ok(())
    }

    /// Retries downloading the file.
    ///
    /// Retries initial request and downloads the entire file once again. If
    /// upon the initial request server signaled support for range requests,
    /// [`download_remaining`](Downloader::download_remaining) is used instead.
    async fn retry(&self, url: &DownloadInfo, file: &mut File) -> Result<(), DownloadError> {
        loop {
            let mut response = request_range_from(url, 0).await?;

            match save_chunks_to_file_at(&mut response, file, 0).await {
                Ok(()) => break,

                Err(SaveChunksError::Network(err)) => {
                    warn!("Error while downloading response: {err}.\nRetrying...");
                    continue;
                }

                Err(SaveChunksError::Io(err)) => {
                    return Err(DownloadError::FromIo {
                        source: err,
                        context: "Error while saving to file".to_string(),
                    })
                }
            }
        }

        Ok(())
    }

    /// Returns the filename.
    pub fn filename(&self) -> &Path {
        self.target_filename.as_path()
    }

    /// Builds a temporary filename the file will be downloaded into.
    async fn temp_filename(&self) -> Result<PathBuf, DownloadError> {
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
        let target_file_path = self.target_filename.clone();
        let file_name = target_file_path
            .file_name()
            .ok_or_else(|| FileError::InvalidFileName(target_file_path.clone()))?
            .to_str()
            .ok_or_else(|| FileError::InvalidFileName(target_file_path.clone()))?;
        let parent_dir = target_file_path
            .parent()
            .ok_or_else(|| FileError::InvalidFileName(target_file_path.clone()))?;

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
async fn request_range_from(
    url: &DownloadInfo,
    range_start: u64,
) -> Result<reqwest::Response, reqwest::Error> {
    // Default retry is an exponential retry with a limit of 15 minutes total.
    // Let's set some more reasonable retry policy so we don't block the downloads for too long.
    let backoff = ExponentialBackoff {
        initial_interval: BACKOFF_INITIAL_INTERVAL,
        max_elapsed_time: Some(BACKOFF_MAX_ELAPSED),
        ..Default::default()
    };

    let operation = || async {
        let mut client = reqwest::Client::new().get(url.url());

        if let Some(Auth::Bearer(token)) = &url.auth {
            client = client.bearer_auth(token)
        }

        if range_start != 0 {
            client = client.header("Range", format!("bytes={range_start}-"));
        }

        client
            .send()
            .await
            .map_err(|err| {
                if err.is_connect() || err.is_builder() {
                    backoff::Error::Permanent(err)
                } else {
                    log::warn!("Failed to Download. {:?}\nRetrying.", &err);
                    backoff::Error::transient(err)
                }
            })?
            .error_for_status()
            .map_err(|err| match err.status() {
                Some(status_error) if status_error.is_client_error() => {
                    backoff::Error::Permanent(err)
                }
                _ => backoff::Error::transient(err),
            })
    };

    retry(backoff, operation).await
}

/// Saves a response body chunks starting from an offset.
async fn save_chunks_to_file_at(
    response: &mut reqwest::Response,
    writer: &mut File,
    offset: u64,
) -> Result<(), SaveChunksError> {
    writer.seek(SeekFrom::Start(offset))?;

    while let Some(bytes) = response.chunk().await? {
        debug!("read response chunk, size={size}", size = bytes.len());
        writer.write_all(&bytes)?;
    }

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
fn try_pre_allocate_space(
    file: &mut File,
    path: &Path,
    file_len: u64,
) -> Result<(), DownloadError> {
    if file_len == 0 {
        return Ok(());
    }

    if let Some(root) = path.parent() {
        let tmpstats =
            statvfs::statvfs(root).context(format!("Can't stat temporary path {root:?}"))?;

        // Reserve 5% of total disk space
        let five_percent_disk_space =
            (tmpstats.blocks() as u64 * tmpstats.block_size() as u64) * 5 / 100;
        let usable_disk_space =
            tmpstats.blocks_free() as u64 * tmpstats.block_size() as u64 - five_percent_disk_space;

        if file_len >= usable_disk_space {
            return Err(DownloadError::InsufficientSpace);
        }

        // Reserve diskspace
        #[cfg(target_os = "linux")]
        let _ = fallocate(
            file.as_raw_fd(),
            FallocateFlags::empty(),
            0,
            file_len.try_into().expect("file too large to fit in i64"),
        );
    }
    Ok(())
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use anyhow::bail;
    use mockito::mock;
    use nix::sys::statvfs;
    use std::io::Write;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;
    use test_case::test_case;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;
    use tokio::net::TcpListener;

    #[test]
    fn construct_downloader_filename() {
        let name = "test_download";
        let version = Some("test1".to_string());
        let target_dir_path = PathBuf::from("/tmp");

        let downloader = Downloader::new_sm(name, &version, target_dir_path);

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

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());
        downloader.download(&url).await?;

        let log_content = std::fs::read(downloader.filename())?;

        assert_eq!("hello".as_bytes(), log_content);

        Ok(())
    }

    #[tokio::test]
    async fn downloader_download_to_target_path() -> anyhow::Result<()> {
        let temp_dir = tempdir()?;
        let _mock1 = mock("GET", "/some_file.txt")
            .with_status(200)
            .with_body(b"hello")
            .create();

        let target_file_path = temp_dir.path().join("downloaded_file.txt");

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&target_file_path, PermissionEntry::default());
        downloader.download(&url).await?;

        let file_content = std::fs::read(target_file_path)?;

        assert_eq!(file_content, "hello".as_bytes());

        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn downloader_download_with_content_length_larger_than_usable_disk_space(
    ) -> anyhow::Result<()> {
        let tmpstats = statvfs::statvfs("/tmp")?;
        let usable_disk_space = tmpstats.blocks_free() * tmpstats.block_size();
        let _mock1 = mock("GET", "/some_file.txt")
            .with_header("content-length", &(usable_disk_space.to_string()))
            .create();

        let name = "test_download_with_length";
        let version = Some("test1".to_string());
        let target_dir_path = TempDir::new()?;

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Err(DownloadError::InsufficientSpace) => Ok(()),
            _ => bail!("failed"),
        }
    }

    #[tokio::test]
    async fn invalid_filename() -> anyhow::Result<()> {
        let temp_dir = tempdir()?;
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let _mock1 = mock("GET", "/some_file.txt")
            .with_status(200)
            .with_body(b"hello")
            .create();

        let name = "test_download";
        let version = Some("test1".to_string());
        let target_dir_path = "";

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new_sm(name, &version, target_dir_path);

        let err = downloader.download(&url).await.unwrap_err();
        assert!(matches!(err, DownloadError::FromIo { .. }));

        downloader.cleanup().await?;

        Ok(())
    }

    #[tokio::test]
    async fn writing_to_existing_file() -> anyhow::Result<()> {
        let temp_dir = tempdir()?;
        let _mock1 = mock("GET", "/some_file.txt")
            .with_status(200)
            .with_body(b"hello")
            .create();

        let target_file_path = temp_dir.path().join("downloaded_file.txt");
        std::fs::File::create(&target_file_path).unwrap();

        let mut target_url = mockito::server_url();
        target_url.push_str("/some_file.txt");

        let url = DownloadInfo::new(&target_url);

        let downloader = Downloader::new(&target_file_path, PermissionEntry::default());
        downloader.download(&url).await?;

        let file_content = std::fs::read(target_file_path)?;

        assert_eq!(file_content, "hello".as_bytes());

        Ok(())
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

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());

        match downloader.download(&url).await {
            Ok(()) => {
                let log_content = std::fs::read(downloader.filename())?;
                let expected_content = std::fs::read(file_path)?;
                assert_eq!(log_content, expected_content);
                Ok(())
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

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());
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

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Ok(()) => {
                assert_eq!("".as_bytes(), std::fs::read(downloader.filename())?);
                Ok(())
            }
            _ => {
                bail!("failed")
            }
        }
    }

    /// This test simulates HTTP response where a connection just drops and a
    /// client hits a timeout, having downloaded only part of the response.
    ///
    /// I couldn't find a reliable way to drop the TCP connection without doing
    /// a closing handshake, so the TCP connection is closed normally, but
    /// because `Transfer-Encoding: chunked` is used, when closing the
    /// connection, the client sees that it hasn't received a 0-length
    /// termination chunk (which signals that the entire HTTP chunked body has
    /// been sent) and retries the request with a `Range` header.
    #[tokio::test]
    async fn resume_download_when_disconnected() -> anyhow::Result<()> {
        let chunk_size = 4;
        let file = "AAAABBBBCCCCDDDD";

        let server_task = tokio::spawn(async move {
            let listener = TcpListener::bind("localhost:3000").await.unwrap();

            while let Ok((mut stream, _addr)) = listener.accept().await {
                let response_task = async move {
                    let (reader, mut writer) = stream.split();
                    let mut lines = BufReader::new(reader).lines();
                    let mut range: Option<std::ops::Range<usize>> = None;

                    // We got an HTTP request, read the lines of the request
                    'inner: while let Ok(Some(line)) = lines.next_line().await {
                        if line.to_ascii_lowercase().contains("range:") {
                            let (_, bytes) = line.split_once('=').unwrap();
                            let (start, end) = bytes.split_once('-').unwrap();
                            let start = start.parse().unwrap_or(0);
                            let end = end.parse().unwrap_or(file.len());
                            range = Some(start..end)
                        }
                        // On `\r\n\r\n` (empty line) stop reading the request
                        // and start responding
                        if line.is_empty() {
                            break 'inner;
                        }
                    }

                    if let Some(range) = range {
                        let start = range.start;
                        let end = range.end;
                        let header = format!(
                            "HTTP/1.1 206 Partial Content\r\n\
                            transfer-encoding: chunked\r\n\
                            connection: close\r\n\
                            content-type: application/octet-stream\r\n\
                            content-range: bytes {start}-{end}/*\r\n\
                            accept-ranges: bytes\r\n"
                        );
                        // answer with range starting 1 byte before what client
                        // requested to ensure it correctly parses content-range
                        // and doesn't just keep writing to where it left off in
                        // the previous request
                        let next = (start - 1 + chunk_size).min(file.len());
                        let body = &file[start..next];

                        let size = body.len();
                        let msg = format!("{header}\r\n{size}\r\n{body}\r\n");
                        debug!("sending message = {msg}");
                        writer.write_all(msg.as_bytes()).await.unwrap();
                    } else {
                        let header = "\
                            HTTP/1.1 200 OK\r\n\
                            transfer-encoding: chunked\r\n\
                            connection: close\r\n\
                            content-type: application/octet-stream\r\n\
                            accept-ranges: bytes\r\n";

                        let body = "AAAA";
                        let msg = format!("{header}\r\n4\r\n{body}\r\n");
                        writer.write_all(msg.as_bytes()).await.unwrap();
                    }
                };
                tokio::spawn(response_task);
            }
        });

        // Wait until task binds a listener on the TCP port
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let tmpdir = TempDir::new()?;
        let downloader = Downloader::new_sm("partial_download", &None, &tmpdir);
        let url = DownloadInfo::new("http://localhost:3000/");

        downloader.download(&url).await?;
        let saved_file = std::fs::read_to_string(downloader.filename())?;
        assert_eq!(saved_file, file);

        downloader.cleanup().await?;

        server_task.abort();

        Ok(())
    }

    // Parameters:
    //
    // - status code
    // - bearer token boolean
    // - maybe url
    // - expected std error
    // - description
    #[test_case(
        200,
        false,
        Some("not_a_url"),
        "URL"
        ; "builder error"
    )]
    #[test_case(
        200,
        true,
        Some("not_a_url"),
        "URL"
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
                DownloadInfo::new(url)
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

        let downloader = Downloader::new_sm(name, &version, target_dir_path.path());
        match downloader.download(&url).await {
            Ok(_success) => anyhow::bail!("Expected client error."),
            Err(err) => {
                // `Error::to_string` uses a Display trait and only contains a
                // top-level error message, and not any lower level contexts. To
                // make sure that we look at the entire error chain, we wrap the
                // error in `anyhow::Error` which reports errors by printing the
                // entire error chain. We can then check keywords that we want
                // appear somewhere in the error chain
                let err = anyhow::Error::from(err);
                println!("{err:?}");

                // We use debug representation because that's what anyhow uses
                // to pretty print error report chain
                assert!(format!("{err:?}")
                    .to_ascii_lowercase()
                    .contains(&expected_err.to_ascii_lowercase()));
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
