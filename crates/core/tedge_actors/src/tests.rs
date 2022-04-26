use crate::runtime::ActorRuntime;
use crate::*;
use async_trait::async_trait;
use futures::lock::Mutex;
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
        &self,
        message: Self::Input,
        output: &mut impl Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        output.send_message(message.to_uppercase()).await
    }
}

type SharedVec<M> = Arc<Mutex<Vec<M>>>;

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
    let mut output = Arc::new(Mutex::new(vec![]));

    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");
    let config = ();
    let mut actor = instantiate::<UppercaseConverter, SharedVec<String>>(&config, output)
        .expect("Fail to activate the plugin");

    actor.run(&runtime);

    //assert_eq!(&expected, output.get_mut());
}
