use anyhow::Result;
use download::DownloadInfo;
use download::Downloader;

/// This example shows how to use the `downlaoder`.
#[tokio::main]
async fn main() -> Result<()> {
    // Create Download metadata.
    let url_data = DownloadInfo::new(
        "https://file-examples-com.github.io/uploads/2017/02/file_example_CSV_5000.csv",
    );

    // Create downloader instance with desired file path and target directory.
    let downloader = Downloader::new("test_download", &None, "/tmp");

    // Call `download` method to get data from url.
    let () = downloader.download(&url_data).await?;

    // Call cleanup method to remove downloaded file if no longer necessary.
    let () = downloader.cleanup().await?;

    Ok(())
}
