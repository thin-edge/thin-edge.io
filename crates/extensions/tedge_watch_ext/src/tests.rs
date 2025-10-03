use crate::WatchActorBuilder;
use crate::WatchEvent;
use crate::WatchRequest;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;

#[tokio::test]
async fn reading_process_stdout() {
    let mut actor = launch_watcher();
    let command = "seq 0 9".to_string();

    actor
        .send(WatchRequest::WatchCommand {
            topic: "seq".to_string(),
            command,
        })
        .await
        .unwrap();
    for i in 0..=9 {
        let msg = actor.recv().await;
        let Some(WatchEvent::NewLine { topic, line }) = msg else {
            panic!("Expecting line from process stdout, got: {:?}", msg);
        };
        assert_eq!(&topic, "seq");
        assert_eq!(line, i.to_string());
    }
    let msg = actor.recv().await;
    let Some(WatchEvent::EndOfStream { topic }) = msg else {
        panic!("Expecting EoS from process stdout, got: {:?}", msg);
    };
    assert_eq!(&topic, "seq");
}

fn launch_watcher() -> SimpleMessageBox<WatchEvent, WatchRequest> {
    let mut client = SimpleMessageBoxBuilder::new("client", 16);
    let mut watcher = WatchActorBuilder::new();
    watcher.connect(&mut client);

    let watcher = watcher.build();
    let client = client.build();
    tokio::spawn(async move {
        let _ = watcher.run().await;
    });
    client
}
