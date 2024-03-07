use crate::test_helpers::ServiceProviderExt;
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
impl Server for SleepService {
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

async fn spawn_sleep_service() -> DynRequestSender<u64, u64> {
    let config = ServerConfig::default();
    let actor = ServerActorBuilder::new(SleepService, &config, Sequential);
    let handle = actor.request_sender();

    tokio::spawn(async move { actor.run().await });

    handle
}

async fn spawn_concurrent_sleep_service(max_concurrency: usize) -> DynRequestSender<u64, u64> {
    let config = ServerConfig::default().with_max_concurrency(max_concurrency);
    let actor = ServerActorBuilder::new(SleepService, &config, Concurrent);
    let handle = actor.request_sender();

    tokio::spawn(async move { actor.run().await });

    handle
}

#[tokio::test]
async fn requests_are_served_in_turn() {
    let mut service_handle = spawn_sleep_service().await;
    let mut client = service_handle.new_client_box(NoConfig);

    // The requests being sent in some order
    client.send(1).await.unwrap();
    client.send(2).await.unwrap();
    client.send(3).await.unwrap();

    // The responses are received in the same order
    assert_eq!(client.recv().await, Some(1));
    assert_eq!(client.recv().await, Some(2));
    assert_eq!(client.recv().await, Some(3));
}

#[tokio::test]
async fn clients_can_interleave_request() {
    let mut service_handle = spawn_sleep_service().await;

    let mut client_1 = service_handle.new_client_box(NoConfig);
    let mut client_2 = service_handle.new_client_box(NoConfig);

    // Two clients can independently send requests
    client_1.send(1).await.unwrap();
    client_2.send(2).await.unwrap();
    client_1.send(3).await.unwrap();

    // The clients receive response to their requests
    assert_eq!(client_1.recv().await, Some(1));
    assert_eq!(client_2.recv().await, Some(2));
    assert_eq!(client_1.recv().await, Some(3));
}

#[tokio::test]
async fn requests_can_be_sent_concurrently() {
    let mut service_handle = spawn_concurrent_sleep_service(2).await;

    let mut client_1 = service_handle.new_client_box(NoConfig);
    let mut client_2 = service_handle.new_client_box(NoConfig);

    // Despite a long running request from client_1
    client_1.send(1000).await.unwrap();
    client_2.send(100).await.unwrap();
    client_2.send(101).await.unwrap();
    client_2.send(102).await.unwrap();

    // Client_2 can use the service
    assert_eq!(client_2.recv().await, Some(100));
    assert_eq!(client_2.recv().await, Some(101));
    assert_eq!(client_2.recv().await, Some(102));
    assert_eq!(client_1.recv().await, Some(1000));
}
