use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;
use futures::StreamExt;
use std::path::PathBuf;
use tracing::error;
use tracing::info;

pub struct DownloadManager {
    http_client: Box<dyn C8YHttpProxy>,
    tmp_dir: PathBuf,
    req_rcvr: UnboundedReceiver<DownloadRequest>,
    res_sndr: UnboundedSender<DownloadResponse>,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub file_name: String,
}

pub type DownloadResult = Result<PathBuf, SMCumulocityMapperError>;

#[derive(Debug)]
pub struct DownloadResponse {
    pub id: String,
    pub result: DownloadResult,
}

impl DownloadRequest {
    pub fn new(id: &str, url: &str, file_name: &str) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            file_name: file_name.into(),
        }
    }
}

impl DownloadResponse {
    pub fn new(id: &str, result: DownloadResult) -> Self {
        Self {
            id: id.into(),
            result,
        }
    }
}

impl DownloadManager {
    pub fn new(
        http_client: Box<dyn C8YHttpProxy>,
        tmp_dir: PathBuf,
        req_rcvr: UnboundedReceiver<DownloadRequest>,
        res_sndr: UnboundedSender<DownloadResponse>,
    ) -> Self {
        Self {
            http_client,
            tmp_dir,
            req_rcvr,
            res_sndr,
        }
    }

    pub async fn run(&mut self) {
        while let Some(req) = self.req_rcvr.next().await {
            let id = req.id;
            let url = req.url;
            let file_name = req.file_name;
            info!("Downloading for req_id: {} from url: {}", id, url);
            let result = self
                .http_client
                .download_file(&url, &file_name, &self.tmp_dir)
                .await;
            info!("Sending download result for req_id: {}", id);
            if (self.res_sndr.send(DownloadResponse::new(&id, result)).await).is_err() {
                error!("Failed to send download response for req_id: {}", id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::DownloadManager;
    use super::DownloadRequest;
    use super::DownloadResponse;
    use assert_matches::assert_matches;
    use c8y_api::http_proxy::MockC8YHttpProxy;
    use c8y_api::smartrest::error::SMCumulocityMapperError;
    use futures::channel::mpsc;
    use futures::channel::mpsc::UnboundedReceiver;
    use futures::channel::mpsc::UnboundedSender;
    use futures::SinkExt;
    use futures::StreamExt;
    use mockall::predicate;
    use mqtt_tests::with_timeout::WithTimeout;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT: Duration = Duration::from_secs(3);

    #[tokio::test]
    async fn download_success() -> anyhow::Result<()> {
        let mut tmp_dir = TempTedgeDir::new();
        let mut c8y_http_proxy = MockC8YHttpProxy::new();
        c8y_http_proxy
            .expect_download_file()
            .with(
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .returning(move |_, file_name, tmp_dir_path| Ok(tmp_dir_path.join(file_name)));

        let (mut req_sndr, mut res_rcvr) =
            start_download_manager(c8y_http_proxy, &mut tmp_dir).await;

        let op_id = "123";
        let mock_http_server_host = mockito::server_url();
        let download_url = format!("{mock_http_server_host}/some/cloud/url");
        let file_name = "test.bin";
        let file_path = tmp_dir.to_path_buf().join(file_name);

        req_sndr
            .send(DownloadRequest::new(op_id, &download_url, "test.bin"))
            .await?;

        let response = res_rcvr
            .next()
            .with_timeout(TEST_TIMEOUT)
            .await?
            .expect("Response expected before timeout");

        assert_eq!(response.id, op_id);
        assert_eq!(
            response.result.expect("Expected result with path"),
            file_path
        );

        Ok(())
    }

    #[tokio::test]
    async fn download_failure() -> anyhow::Result<()> {
        let mut tmp_dir = TempTedgeDir::new();
        let mut c8y_http_proxy = MockC8YHttpProxy::new();
        c8y_http_proxy
            .expect_download_file()
            .with(
                predicate::always(),
                predicate::always(),
                predicate::always(),
            )
            .returning(move |_, _, _| Err(SMCumulocityMapperError::RequestTimeout));

        let (mut req_sndr, mut res_rcvr) =
            start_download_manager(c8y_http_proxy, &mut tmp_dir).await;

        let op_id = "123";
        let mock_http_server_host = mockito::server_url();
        let download_url = format!("{mock_http_server_host}/some/cloud/url");

        req_sndr
            .send(DownloadRequest::new(op_id, &download_url, "test.bin"))
            .await?;

        let response = res_rcvr
            .next()
            .with_timeout(TEST_TIMEOUT)
            .await?
            .expect("Response expected before timeout");

        assert_eq!(response.id, op_id);
        assert_matches!(
            response.result,
            Err(SMCumulocityMapperError::RequestTimeout)
        );

        Ok(())
    }

    pub async fn start_download_manager(
        c8y_http_proxy: MockC8YHttpProxy,
        tmp_dir: &mut TempTedgeDir,
    ) -> (
        UnboundedSender<DownloadRequest>,
        UnboundedReceiver<DownloadResponse>,
    ) {
        let (req_sndr, req_rcvr) = mpsc::unbounded::<DownloadRequest>();
        let (res_sndr, res_rcvr) = mpsc::unbounded::<DownloadResponse>();

        let mut download_manager = DownloadManager::new(
            Box::new(c8y_http_proxy),
            tmp_dir.to_path_buf(),
            req_rcvr,
            res_sndr,
        );

        tokio::spawn(async move { download_manager.run().await });

        (req_sndr, res_rcvr)
    }
}
