use crate::runtime::ActorRuntime;
use crate::*;
use async_trait::async_trait;

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
        output: &mut Recipient<String>,
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
    let mut output: MailBox<String> = MailBox::new();

    let mut actor = instance::<UppercaseConverter>(&()).expect("Fail to build the actor");
    let mut source = instance::<Vec<String>>(&input).expect("Fail to build a source from input");

    actor.set_recipient(output.get_address().into());
    source.set_recipient(actor.address().into());

    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");

    futures::executor::block_on(async {
        runtime.run(source).await;
        runtime.run(actor).await;

        let mut expected = expected.into_iter();
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());

        // TODO Handle end of input
        // assert_eq!(output.next_message().await, None);
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
