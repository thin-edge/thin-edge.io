use crate::SetTimeout;
use crate::Timeout;
use crate::TimerActor;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::Message;
use tedge_actors::MessageReceiver;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;

#[tokio::test]
async fn timeout_requests_lead_to_chronological_timeout_responses() {
    let mut client_box_builder = SimpleMessageBoxBuilder::new("Test timers", 16);
    let _signal_handler = spawn_timer_actor(&mut client_box_builder).await;
    let mut client_box = client_box_builder.build();

    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(1000),
            event: "Do X".to_string(),
        })
        .await
        .unwrap();

    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(500),
            event: "This needs to be done before X".to_string(),
        })
        .await
        .unwrap();

    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(100),
            event: "Do this asap".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(
        client_box.recv().await,
        Some(Timeout {
            event: "Do this asap".to_string()
        })
    );
    assert_eq!(
        client_box.recv().await,
        Some(Timeout {
            event: "This needs to be done before X".to_string()
        })
    );
    assert_eq!(
        client_box.recv().await,
        Some(Timeout {
            event: "Do X".to_string()
        })
    );
}

#[tokio::test]
async fn should_shutdown_even_if_there_are_pending_timers() {
    let mut client_box_builder = SimpleMessageBoxBuilder::new("Test timers", 16);
    let mut signal_handler = spawn_timer_actor(&mut client_box_builder).await;
    let mut client_box = client_box_builder.build();

    // Send a long running timer.
    client_box
        .send(SetTimeout {
            duration: Duration::from_secs(5),
            event: "Take your time".to_string(),
        })
        .await
        .unwrap();

    // Then send a short one, to be sure the timer actor is actually running
    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(5),
            event: "Asap".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(
        client_box.recv().await,
        Some(Timeout {
            event: "Asap".to_string()
        })
    );

    // Sent a graceful shutdown request
    signal_handler.send(RuntimeRequest::Shutdown).await.unwrap();

    // The actor timer is expected to shutdown immediately
    assert_eq!(
        Ok(None),
        tokio::time::timeout(Duration::from_millis(100), client_box.recv()).await
    );
}

#[tokio::test]
async fn should_process_all_pending_timers_on_end_of_inputs() {
    let mut client_box_builder = SimpleMessageBoxBuilder::new("Test timers", 16);
    let _ = spawn_timer_actor(&mut client_box_builder).await;
    let mut client_box = client_box_builder.build();

    // Send some timeout requests
    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(200),
            event: 1,
        })
        .await
        .unwrap();

    client_box
        .send(SetTimeout {
            duration: Duration::from_millis(100),
            event: 2,
        })
        .await
        .unwrap();

    // Then close the stream of requests
    client_box.close_sender();

    // The actor timer is expected to shutdown,
    // but *only* after all the pending requests have been processed.
    assert_eq!(client_box.recv().await, Some(Timeout { event: 2 }));
    assert_eq!(client_box.recv().await, Some(Timeout { event: 1 }));
    assert_eq!(
        Ok(None),
        tokio::time::timeout(Duration::from_millis(100), client_box.recv()).await
    );
}

async fn spawn_timer_actor<T: Message>(
    peer: &mut impl ServiceConsumer<SetTimeout<T>, Timeout<T>, NoConfig>,
) -> DynSender<RuntimeRequest> {
    let mut builder = TimerActor::builder();
    builder.add_peer(peer);
    let signal_sender = builder.get_signal_sender();

    tokio::spawn(async move {
        let mut actor = builder.build();
        let _ = actor.run().await;
    });

    signal_sender
}
