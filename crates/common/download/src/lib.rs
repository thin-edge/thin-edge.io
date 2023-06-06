//! Utilities for managing file downloads.
//!
//! This crate simplifies managing and tracking file downloads by:
//!
//! - using a single downloader to download related files
//! - cleaning downloaded files when they're no longer necessary
//! - implementing reasonable exponential backoff strategy
//! - performing partial downloads if a portion of a file has already been
//!   downloaded
//!
//! # Usage
//!
//! First, a [`Downloader`] has to be created, which will download files to a
//! specified directory, saving them according to a specified pattern. Then to
//! download a file, we can use the [`Downloader::download`] method, passing in
//! a [`DownloadInfo`] which describes the request used to retrieve a file. When
//! files downloaded by a downloader are no longer necessary,
//! [`Downloader::cleanup`] method can delete the downloaded files.
//!
//! ```rust
//! use anyhow::Result;
//! use download::DownloadInfo;
//! use download::Downloader;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Create Download metadata.
//!     let url_data = DownloadInfo::new(
//!         "https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh",
//!     );
//!
//!     // Create downloader instance with desired file path and target directory.
//!     let downloader = Downloader::new("/tmp/test_download".into());
//!
//!     // Call `download` method to get data from url.
//!     downloader.download(&url_data).await?;
//!
//!     // Call cleanup method to remove downloaded file if no longer necessary.
//!     downloader.cleanup().await?;
//!
//!     Ok(())
//! }
//! ```

mod download;
mod error;

pub use crate::download::Auth;
pub use crate::download::DownloadInfo;
pub use crate::download::Downloader;
pub use crate::error::DownloadError;
