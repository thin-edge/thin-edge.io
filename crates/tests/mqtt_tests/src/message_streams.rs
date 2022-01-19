use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;

/// A `Sink` of `T` that populates a vector of `T`s on closed.
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
///     use futures::SinkExt;
///
///     let (output, mut output_sink) = mqtt_tests::output_stream();
///     tokio::spawn(async move {
///         output_sink.send(1).await;
///         output_sink.send(2).await;
///         output_sink.send(3).await;
///     });
///     assert_eq!(vec![1,2,3], output.collect().await);
/// # }
/// ```
pub fn output_stream<T>() -> (MessageOutputStream<T>, mpsc::UnboundedSender<T>) {
    let (sender, receiver) = mpsc::unbounded();
    let recorder = MessageOutputStream { receiver };
    (recorder, sender)
}

pub struct MessageOutputStream<T> {
    receiver: mpsc::UnboundedReceiver<T>,
}

impl<T> MessageOutputStream<T> {
    pub async fn collect(mut self) -> Vec<T> {
        let mut result = vec![];
        while let Some(item) = self.receiver.next().await {
            result.push(item);
        }
        result
    }
}

/// A `Stream` of `T` that is populated using a vector of `T` samples.
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
///     use futures::StreamExt;
///
///     let mut input_stream = mqtt_tests::input_stream(vec![
///         1,
///         2,
///         3,
///     ]).await;
///
///     assert_eq!(Some(1), input_stream.next().await);
///     assert_eq!(Some(2), input_stream.next().await);
///     assert_eq!(Some(3), input_stream.next().await);
///     assert_eq!(None, input_stream.next().await);
/// # }
/// ```
pub async fn input_stream<T>(items: Vec<T>) -> mpsc::UnboundedReceiver<T> {
    let (mut sender, receiver) = mpsc::unbounded();
    for item in items {
        let _ = sender.send(item).await;
    }
    receiver
}
