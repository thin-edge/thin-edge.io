use anyhow::Result;
use download::DownloadInfo;
use download::Downloader;
use certificate::CloudRootCerts;

/// This example shows how to use the `downloader`.
#[tokio::main]
async fn main() -> Result<()> {
    // Create Download metadata.
    let url_data = DownloadInfo::new(
        "https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/README.md",
    );

    // Create downloader instance with desired file path and target directory.
    #[allow(deprecated)]
    let downloader = Downloader::new("/tmp/test_download".into(), None, CloudRootCerts::from([]));

    // Call `download` method to get data from url.
    downloader.download(&url_data).await?;

    // Call cleanup method to remove downloaded file if no longer necessary.
    downloader.cleanup().await?;

    Ok(())
}
