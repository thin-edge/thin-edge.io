use crate::runtime::ActorRuntime;
use crate::*;
use async_trait::async_trait;
use futures::lock::Mutex;
use std::ops::Deref;
use std::sync::Arc;

/// An actor that converts string messages to uppercase
struct UppercaseConverter;

#[async_trait]
impl Actor for UppercaseConverter {
    type Config = ();
    type Input = String;
    type Output = String;
    type Producer = DevNull;
    type Reactor = Self;

    fn try_new(_config: &Self::Config) -> Result<Self, RuntimeError> {
        Ok(UppercaseConverter)
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((DevNull, self))
    }
}

#[async_trait]
impl Reactor<String, String> for UppercaseConverter {
    async fn react(
        &mut self,
        message: String,
        output: &mut impl Recipient<String>,
    ) -> Result<(), RuntimeError> {
        output.send_message(message.to_uppercase()).await
    }
}

#[test]
fn it_works() {
    let input: Vec<String> = vec!["foo", "bar", "zoo"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let expected: Vec<String> = vec!["FOO", "BAR", "ZOO"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let output = Arc::new(Mutex::new(vec![]));

    let actor = instance::<UppercaseConverter>(&())
        .expect("Fail to build the actor")
        .with_recipient(output.clone());
    let source = instance::<Vec<String>>(&input)
        .expect("Fail to build a source from input")
        .with_recipient(actor.address());

    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");

    futures::executor::block_on(async {
        runtime.run(source).await;
        runtime.run(actor).await;

        std::thread::sleep(std::time::Duration::from_secs(1));

        let output = output.lock().await;
        assert_eq!(&expected, output.deref());
    })
}

#[derive(Clone, Debug)]
struct Msg1 {
    x: i32,
}
#[derive(Clone, Debug)]
enum Msg2 {
    A,
    B,
    C,
}
#[derive(Clone, Debug)]
struct Msg3 {
    x: String,
}

message_type!(Msg[Msg1,Msg2,Msg3]);

#[test]
fn creating_message_type() {
    let msg = Msg1 { x: 42 };
    let res = match msg.into() {
        Msg::Msg1(_) => 1,
        Msg::Msg2(_) => 2,
        Msg::Msg3(_) => 3,
    };
    assert_eq!(res, 1);
}
