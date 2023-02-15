use crate::SetTimeout;
use crate::Timeout;
use crate::TimerActor;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::Message;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::SimpleMessageBoxBuilder;

#[tokio::test]
async fn timeout_requests_lead_to_chronological_timeout_responses() {
    let mut client_box_builder = SimpleMessageBoxBuilder::new("Test timers", 16);
    let _ = spawn_timer_actor(&mut client_box_builder).await;
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

async fn spawn_timer_actor<T: Message>(peer: &mut impl MessageBoxPlug<SetTimeout<T>, Timeout<T>>) {
    let mut builder = TimerActor::builder();
    builder.connect_with(peer, NoConfig);

    tokio::spawn(async move {
        let (actor, actor_box) = builder.build();
        let _ = actor.run(actor_box).await;
    });
}
