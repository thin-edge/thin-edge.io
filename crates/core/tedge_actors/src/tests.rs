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

    fn try_new(_config: &Self::Config) -> Result<Self, RuntimeError> {
        Ok(UppercaseConverter)
    }

    fn event_source(&self) -> Self::Producer {
        DevNull
    }

    async fn react(
        &mut self,
        message: Self::Input,
        output: &mut impl Recipient<Self::Output>,
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

    runtime.run(source);
    runtime.run(actor);

    std::thread::sleep(std::time::Duration::from_secs(1));
    futures::executor::block_on(async {
        let output = output.lock().await;
        assert_eq!(&expected, output.deref());
    })
}
