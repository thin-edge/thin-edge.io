use super::*;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
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
                    let msg = format!("{header}\r\n{size:x}\r\n{body}\r\n");
                    // if this is the last chunk, send also terminating 0-length chunk
                    let msg = if next == file.len() {
                        format!("{msg}0\r\n\r\n")
                    } else {
                        msg
                    };
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

/// If after retrying ETag of the resource is different, we should download it
/// from scratch again.
#[tokio::test]
async fn resume_download_with_etag_changed() {
    let file_v1 = "AAAABBBBCCCCDDDD";
    let file_v2 = "XXXXYYYYZZZZWWWW";
    let chunk_size = 4;

    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // server should do 3 things:
    // - only send a portion of the first request
    // - after the first request update the resource
    // - serve 2nd request normally (but we expect it to be a range request)
    let server_task = tokio::spawn(async move {
        let mut request_count = 0;
        while let Ok((mut stream, _addr)) = listener.accept().await {
            let response_task = async move {
                let (reader, mut writer) = stream.split();
                let mut lines = BufReader::new(reader).lines();
                let mut range: Option<std::ops::Range<usize>> = None;

                // We got an HTTP request, read the lines of the request
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.to_ascii_lowercase().contains("range:") {
                        let (_, bytes) = line.split_once('=').unwrap();
                        let (start, end) = bytes.split_once('-').unwrap();
                        let start = start.parse().unwrap_or(0);
                        let end = end.parse().unwrap_or(file_v2.len());
                        range = Some(start..end)
                    }
                    // On `\r\n\r\n` (empty line) stop reading the request
                    // and start responding
                    if line.is_empty() {
                        break;
                    }
                }

                let file = if request_count == 0 { file_v1 } else { file_v2 };
                let etag = if request_count == 0 {
                    "v1-initial"
                } else {
                    "v2-changed"
                };

                if let Some(range) = range {
                    // Return range for both first and subsequent requests
                    let start = range.start;
                    let end = range.end.min(file.len());
                    let header = format!(
                        "HTTP/1.1 206 Partial Content\r\n\
                        transfer-encoding: chunked\r\n\
                        connection: close\r\n\
                        content-type: application/octet-stream\r\n\
                        content-range: bytes {start}-{end}/*\r\n\
                        accept-ranges: bytes\r\n\
                        etag: \"{etag}\"\r\n"
                    );
                    let body = &file[start..end];
                    let size = body.len();
                    let msg = format!("{header}\r\n{size:x}\r\n{body}\r\n0\r\n\r\n");
                    writer.write_all(msg.as_bytes()).await.unwrap();
                    writer.flush().await.unwrap();
                } else {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\n\
                        transfer-encoding: chunked\r\n\
                        connection: close\r\n\
                        content-type: application/octet-stream\r\n\
                        accept-ranges: bytes\r\n\
                        etag: \"{etag}\"\r\n"
                    );

                    let body = &file[0..chunk_size];
                    let size = body.len();
                    let msg = format!("{header}\r\n{size:x}\r\n{body}\r\n");
                    writer.write_all(msg.as_bytes()).await.unwrap();
                    writer.flush().await.unwrap();
                    // Connection drops here without sending final chunk
                }
            };
            request_count += 1;
            tokio::spawn(response_task);
        }
    });

    // Wait until task binds a listener on the TCP port
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let tmpdir = TempDir::new().unwrap();
    let target_path = tmpdir.path().join("partial_download_etag");

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    let url = DownloadInfo::new(&format!("http://localhost:{port}/"));

    downloader.download(&url).await.unwrap();
    let saved_file = std::fs::read_to_string(downloader.filename()).unwrap();
    // Should have the complete new file content since ETag changed
    assert_eq!(saved_file, file_v2);

    downloader.cleanup().await.unwrap();

    server_task.abort();
}

/// If after retrying ETag of the resource is different, we should download it
/// from scratch again.
#[tokio::test]
async fn resumed_download_doesnt_leave_leftovers() {
    let file_v1 = "AAAABBBBCCCCDDDD";
    let file_v2 = "XXXXYYYY";

    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // server should do 3 things:
    // - only send a portion of the first request
    // - after the first request update the resource
    // - serve 2nd request normally (but we expect it to be a range request)
    let server_task = tokio::spawn(async move {
        let mut request_count = 0;
        while let Ok((mut stream, _addr)) = listener.accept().await {
            let response_task = async move {
                let (reader, mut writer) = stream.split();
                let mut lines = BufReader::new(reader).lines();
                let mut range: Option<std::ops::Range<usize>> = None;

                // We got an HTTP request, read the lines of the request
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.to_ascii_lowercase().contains("range:") {
                        let (_, bytes) = line.split_once('=').unwrap();
                        let (start, end) = bytes.split_once('-').unwrap();
                        let start = start.parse().unwrap_or(0);
                        let end = end.parse().unwrap_or(file_v2.len());
                        range = Some(start..end)
                    }
                    // On `\r\n\r\n` (empty line) stop reading the request
                    // and start responding
                    if line.is_empty() {
                        break;
                    }
                }

                let file = if request_count == 0 { file_v1 } else { file_v2 };
                let etag = if request_count == 0 {
                    "v1-initial"
                } else {
                    "v2-changed"
                };

                if let Some(range) = range {
                    let start = range.start;
                    let end = range.end;
                    let msg = if start >= end || end >= file.len() {
                        let header = format!(
                            "HTTP/1.1 416 Range Not Satisfiable\r\n\
                            content-range: bytes */{}\r\n",
                            file.len()
                        );
                        format!("{header}\r\n")
                    } else {
                        // Return range for both first and subsequent requests
                        let header = format!(
                            "HTTP/1.1 206 Partial Content\r\n\
                            transfer-encoding: chunked\r\n\
                            connection: close\r\n\
                            content-type: application/octet-stream\r\n\
                            content-range: bytes {start}-{end}/*\r\n\
                            accept-ranges: bytes\r\n\
                            etag: \"{etag}\"\r\n"
                        );
                        let body = &file[start..end];
                        let size = body.len();
                        format!("{header}\r\n{size:x}\r\n{body}\r\n0\r\n\r\n")
                    };

                    writer.write_all(msg.as_bytes()).await.unwrap();
                    writer.flush().await.unwrap();
                } else {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\n\
                        transfer-encoding: chunked\r\n\
                        connection: close\r\n\
                        content-type: application/octet-stream\r\n\
                        accept-ranges: bytes\r\n\
                        etag: \"{etag}\"\r\n"
                    );

                    let msg = if request_count == 0 {
                        let body = &file[0..12];
                        let size = body.len();
                        format!("{header}\r\n{size:x}\r\n{body}\r\n")
                    } else {
                        let body = file;
                        let size = body.len();
                        format!("{header}\r\n{size:x}\r\n{body}\r\n0\r\n\r\n")
                    };
                    dbg!(&msg);
                    writer.write_all(msg.as_bytes()).await.unwrap();
                    writer.flush().await.unwrap();
                    // Connection drops here without sending final chunk
                }
            };
            request_count += 1;
            tokio::spawn(response_task);
        }
    });

    // Wait until task binds a listener on the TCP port
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let tmpdir = TempDir::new().unwrap();
    let target_path = tmpdir.path().join("partial_download_remains");

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    let url = DownloadInfo::new(&format!("http://localhost:{port}/"));

    downloader.download(&url).await.unwrap();
    let saved_file = std::fs::read_to_string(downloader.filename()).unwrap();
    // Should have the complete new file content since ETag changed
    assert_eq!(saved_file, file_v2);

    downloader.cleanup().await.unwrap();

    server_task.abort();
}

#[tokio::test]
async fn resume_max_5_times() {
    let request_count = Arc::new(AtomicUsize::new(0));
    let rc = request_count.clone();

    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_task = tokio::spawn(async move {
        while let Ok((mut stream, _addr)) = listener.accept().await {
            let response_task = async move {
                let (_, mut writer) = stream.split();
                // Always respond with only first chunk, never completing the response, triggering retries
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
            };
            tokio::spawn(response_task);
            rc.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Wait until task binds a listener on the TCP port
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let tmpdir = TempDir::new().unwrap();
    let target_path = tmpdir.path().join("partial_download");

    let downloader = Downloader::new(target_path, None, CloudHttpConfig::test_value());
    let url = DownloadInfo::new(&format!("http://localhost:{port}/"));

    let err = downloader.download(&url).await.unwrap_err();
    assert!(matches!(err, DownloadError::Request(_)));
    assert!(err.to_string().contains("error decoding response body"));

    downloader.cleanup().await.unwrap();

    server_task.abort();

    assert_eq!(request_count.load(Ordering::SeqCst), 5);
}

// If we succeed before max retries, we should not do more requests.
#[tokio::test]
async fn only_retry_until_success() {
    let file = "AAAABBBBCCCCDDDD";
    let request_count = Arc::new(AtomicUsize::new(0));
    let rc = request_count.clone();

    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_task = tokio::spawn(async move {
        while let Ok((mut stream, _addr)) = listener.accept().await {
            let rc_num = rc.load(Ordering::SeqCst);
            let response_task = async move {
                let (_, mut writer) = stream.split();
                // Always respond with only first chunk, never completing the response, triggering retries
                let header = "\
                            HTTP/1.1 200 OK\r\n\
                            transfer-encoding: chunked\r\n\
                            connection: close\r\n\
                            content-type: application/octet-stream\r\n\
                            accept-ranges: bytes\r\n";

                // On the 2nd request, send the full response, should trigger only 1 retry
                let msg = if rc_num == 0 {
                    let body = "AAAA";
                    format!("{header}\r\n4\r\n{body}\r\n")
                } else {
                    let body = file;
                    let len = file.len();
                    format!("{header}\r\n{len:x}\r\n{body}\r\n0\r\n\r\n")
                };

                writer.write_all(msg.as_bytes()).await.unwrap();
                writer.flush().await.unwrap();
            };
            tokio::spawn(response_task);
            rc.fetch_add(1, Ordering::SeqCst);
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

    assert_eq!(request_count.load(Ordering::SeqCst), 2);
}
