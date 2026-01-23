use super::*;

use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;

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
async fn resume_download_when_disconnected() {
    let chunk_size = 4;
    let file = "AAAABBBBCCCCDDDD";

    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_task = tokio::spawn(async move {
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
                    writer.flush().await.unwrap();
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
                    writer.flush().await.unwrap();
                }
            };
            tokio::spawn(response_task);
        }
    });

    // Wait until task binds a listener on the TCP port
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let tmpdir = TempDir::new().unwrap();
    let target_path = tmpdir.path().join("partial_download");

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    let url = DownloadInfo::new(&format!("http://localhost:{port}/"));

    downloader.download(&url).await.unwrap();
    let saved_file = std::fs::read_to_string(downloader.filename()).unwrap();
    assert_eq!(saved_file, file);

    downloader.cleanup().await.unwrap();

    server_task.abort();
}
