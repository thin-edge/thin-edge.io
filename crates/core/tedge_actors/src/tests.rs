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

    fn try_new(_config: Self::Config) -> Result<Self, RuntimeError> {
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
    ) -> Result<Option<Box<dyn Task>>, RuntimeError> {
        output.send_message(message.to_uppercase()).await?;
        Ok(None)
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

    let mut actor = instance::<UppercaseConverter>(());
    let mut source = instance::<Vec<String>>(input);

    actor.set_recipient(output.get_address().into());
    source.set_recipient(actor.address().into());

    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");

    futures::executor::block_on(async {
        runtime
            .run(source)
            .await
            .expect("Fail to produce input from source");
        runtime.run(actor).await.expect("Fail to run the actor");

        let mut expected = expected.into_iter();
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());

        // TODO Handle end of input
        // assert_eq!(output.next_message().await, None);
    })
}

/// An actor that converts string messages to uppercase, but send the responses later
struct AsyncUppercaseConverter;

#[async_trait]
impl Actor for AsyncUppercaseConverter {
    type Config = ();
    type Input = String;
    type Output = String;
    type Producer = DevNull;
    type Reactor = Self;

    fn try_new(_config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(AsyncUppercaseConverter)
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((DevNull, self))
    }
}

#[async_trait]
impl Reactor<String, String> for AsyncUppercaseConverter {
    async fn react(
        &mut self,
        message: String,
        output: &mut Recipient<String>,
    ) -> Result<Option<Box<dyn Task>>, RuntimeError> {
        let task = UppercaseTask {
            input: message,
            output: output.clone(),
        };

        Ok(Some(Box::new(task)))
    }
}

struct UppercaseTask {
    input: String,
    output: Recipient<String>,
}

#[async_trait]
impl Task for UppercaseTask {
    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let input = self.input;
        let result = input.to_uppercase();

        self.output.send_message(result).await
    }
}

#[test]
fn output_messages_can_be_sent_asynchronously() {
    let input: Vec<String> = vec!["foo", "bar", "zoo"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let expected: Vec<String> = vec!["FOO", "BAR", "ZOO"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let mut output: MailBox<String> = MailBox::new();

    let mut actor = instance::<AsyncUppercaseConverter>(());
    let mut source = instance::<Vec<String>>(input);

    actor.set_recipient(output.get_address().into());
    source.set_recipient(actor.address().into());

    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");

    futures::executor::block_on(async {
        runtime
            .run(source)
            .await
            .expect("Fail to produce input from source");
        runtime.run(actor).await.expect("Fail to run the actor");
        let mut expected = expected.into_iter();
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());
        assert_eq!(output.next_message().await, expected.next());

        // TODO Handle end of input
        // assert_eq!(output.next_message().await, None);
    })
}

#[derive(Clone, Debug)]
pub struct Msg1 {
    x: i32,
}
#[derive(Clone, Debug)]
pub enum Msg2 {
    A,
    B,
    C,
}
#[derive(Clone, Debug)]
pub struct Msg3 {
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
