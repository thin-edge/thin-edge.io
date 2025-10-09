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
    let mut actor = launch_watcher(1).pop().unwrap();
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
        let Some(WatchEvent::StdoutLine { topic, line }) = msg else {
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

#[tokio::test]
async fn serving_independent_clients() {
    let mut actors = launch_watcher(2);
    let mut actor_1 = actors.pop().unwrap();
    let mut actor_2 = actors.pop().unwrap();
    let command_1 = "seq 0 9".to_string();
    let command_2 = "seq 0 10 90".to_string();

    actor_1
        .send(WatchRequest::WatchCommand {
            topic: "seq".to_string(),
            command: command_1,
        })
        .await
        .unwrap();
    actor_2
        .send(WatchRequest::WatchCommand {
            topic: "seq".to_string(),
            command: command_2,
        })
        .await
        .unwrap();

    for i in 0..=9 {
        let msg_1 = actor_1.recv().await;
        let Some(WatchEvent::StdoutLine { topic, line }) = msg_1 else {
            panic!("Expecting line from process stdout, got: {:?}", msg_1);
        };
        assert_eq!(&topic, "seq");
        assert_eq!(line, i.to_string());

        let msg_2 = actor_2.recv().await;
        let Some(WatchEvent::StdoutLine { topic, line }) = msg_2 else {
            panic!("Expecting line from process stdout, got: {:?}", msg_2);
        };
        assert_eq!(&topic, "seq");
        assert_eq!(line, (i * 10).to_string());
    }
    let msg_1 = actor_1.recv().await;
    let Some(WatchEvent::EndOfStream { topic }) = msg_1 else {
        panic!("Expecting EoS from process stdout, got: {:?}", msg_1);
    };
    assert_eq!(&topic, "seq");
    let msg_2 = actor_2.recv().await;
    let Some(WatchEvent::EndOfStream { topic }) = msg_2 else {
        panic!("Expecting EoS from process stdout, got: {:?}", msg_2);
    };
    assert_eq!(&topic, "seq");
}

fn launch_watcher(client_count: u32) -> Vec<SimpleMessageBox<WatchEvent, WatchRequest>> {
    let mut watcher = WatchActorBuilder::new();
    let clients = (0..=client_count)
        .map(|id| SimpleMessageBoxBuilder::new(&format!("client {id}"), 16))
        .map(|mut client| {
            watcher.connect(&mut client);
            client.build()
        })
        .collect::<Vec<_>>();

    let watcher = watcher.build();
    tokio::spawn(async move {
        let _ = watcher.run().await;
    });

    clients
}
