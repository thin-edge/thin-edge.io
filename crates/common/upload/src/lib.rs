//! Utilities for managing file uploads.
//!
//! This crate simplifies managing file uploads by:
//!
//! - using a single uploader to upload related files
//! - implementing reasonable exponential backoff strategy
//!
//! # Usage
//!
//! First, a [`Uploader`] has to be created, which contains
//! information about location of the file we want upload to given url.
//! To upload a file, we can use the [`Uploader::upload`] method, passing in
//! a [`UploadInfo`] which describes the request used to send a file.
//!
//! ```no_run
//! use upload::UploadInfo;
//! use upload::Uploader;
//! use upload::UploadError;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), UploadError> {
//!     // Create Upload metadata.
//!     let url_data = UploadInfo::new(
//!         "https://example.com/destination_file",
//!     );
//!
//!     let identity = unimplemented!("Get client certificate from configuration");
//!     // Create uploader instance with source file path.
//!     let uploader = Uploader::new("/tmp/test_upload".into(), identity);
//!
//!     // Call `upload` method to send data to url.
//!     uploader.upload(&url_data).await?;
//!
//!     Ok(())
//! }
//! ```
//!
mod error;
mod upload;

pub use crate::error::UploadError;
pub use crate::upload::Auth;
pub use crate::upload::ContentType;
pub use crate::upload::FormData;
pub use crate::upload::UploadInfo;
pub use crate::upload::UploadMethod;
pub use crate::upload::Uploader;
pub use mime::Mime;
