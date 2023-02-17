use crate::*;
use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

/// An actor used to test the message box for concurrent services.
///
/// This actor processes basic messages (a simple number of milli seconds),
/// and waits that duration before returning an echo of the request to the sender.
#[derive(Clone)]
struct SleepService;

#[async_trait]
impl Service for SleepService {
    type Request = u64;
    // A number a milli seconds to wait before returning a response
    type Response = u64; // An echo of the request

    fn name(&self) -> &str {
        "ConcurrentWorker"
    }

    async fn handle(&mut self, request: u64) -> u64 {
        sleep(Duration::from_millis(request)).await;
        request
    }
}

async fn spawn_sleep_service() -> SimpleMessageBox<(ClientId, u64), (ClientId, u64)> {
    let service = SleepService;
    let actor = ServiceActor::new(service);
    let (handle, messages) = SimpleMessageBox::channel(actor.name(), 16);

    tokio::spawn(actor.run(messages));

    handle
}

async fn spawn_concurrent_sleep_service(
    max_concurrency: usize,
) -> SimpleMessageBox<(ClientId, u64), (ClientId, u64)> {
    let service = SleepService;
    let actor = ConcurrentServiceActor::new(service);
    let (handle, messages) =
        ConcurrentServiceMessageBox::channel(actor.name(), 16, max_concurrency);

    tokio::spawn(actor.run(messages));

    handle
}

#[tokio::test]
async fn requests_are_served_in_turn() {
    let mut service_handle = spawn_sleep_service().await;

    let client = 1;

    // The requests being sent in some order
    service_handle.send((client, 1)).await.unwrap();
    service_handle.send((client, 2)).await.unwrap();
    service_handle.send((client, 3)).await.unwrap();

    // The responses are received in the same order
    assert_eq!(service_handle.recv().await, Some((client, 1)));
    assert_eq!(service_handle.recv().await, Some((client, 2)));
    assert_eq!(service_handle.recv().await, Some((client, 3)));
}

#[tokio::test]
async fn clients_can_interleave_request() {
    let mut service_handle = spawn_sleep_service().await;

    let client_1 = 1;
    let client_2 = 2;

    // Two clients can independently send requests
    service_handle.send((client_1, 1)).await.unwrap();
    service_handle.send((client_2, 2)).await.unwrap();
    service_handle.send((client_1, 3)).await.unwrap();

    // The clients receive response to their requests
    assert_eq!(service_handle.recv().await, Some((client_1, 1)));
    assert_eq!(service_handle.recv().await, Some((client_2, 2)));
    assert_eq!(service_handle.recv().await, Some((client_1, 3)));
}

#[tokio::test]
async fn requests_can_be_sent_concurrently() {
    let mut service_handle = spawn_concurrent_sleep_service(2).await;

    let client_1 = 1;
    let client_2 = 2;

    // Despite a long running request from client_1
    service_handle.send((client_1, 1000)).await.unwrap();
    service_handle.send((client_2, 100)).await.unwrap();
    service_handle.send((client_2, 101)).await.unwrap();
    service_handle.send((client_2, 102)).await.unwrap();

    // Client_2 can use the service
    assert_eq!(service_handle.recv().await, Some((client_2, 100)));
    assert_eq!(service_handle.recv().await, Some((client_2, 101)));
    assert_eq!(service_handle.recv().await, Some((client_2, 102)));
    assert_eq!(service_handle.recv().await, Some((client_1, 1000)));
}
