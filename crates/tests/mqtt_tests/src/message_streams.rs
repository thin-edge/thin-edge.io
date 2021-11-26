use core::pin::Pin;
use core::task::{Context, Poll};
use futures::channel::oneshot;
use futures::{Sink, Stream};

pub struct MessageOutputStream<T>(Option<StreamRecorder<T>>);

struct StreamRecorder<T> {
    messages: Vec<T>,
    sender: oneshot::Sender<Vec<T>>,
}

pub fn recorder<T>() -> (oneshot::Receiver<Vec<T>>, MessageOutputStream<T>) {
    let (output_sender, output_receiver) = oneshot::channel();
    let recorder = StreamRecorder {
        messages: vec![],
        sender: output_sender,
    };
    let output_stream = MessageOutputStream(Some(recorder));
    (output_receiver, output_stream)
}

impl<T> Sink<T> for MessageOutputStream<T> {
    type Error = core::convert::Infallible;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if let Some(recorder) = unsafe { self.get_unchecked_mut() }.0.as_mut() {
            recorder.messages.push(item);
        }
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Some(recorder) = unsafe { self.get_unchecked_mut() }.0.take() {
            let messages = recorder.messages;
            let _ = recorder.sender.send(messages);
        }
        Poll::Ready(Ok(()))
    }
}

pub struct MessageInputStream<T> {
    items: std::vec::IntoIter<T>,
}

impl<T> MessageInputStream<T> {
    pub fn new(items: Vec<T>) -> MessageInputStream<T> {
        MessageInputStream {
            items: items.into_iter(),
        }
    }
}

unsafe impl<T> Send for MessageInputStream<T> {}

impl<T> Stream for MessageInputStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = unsafe { self.get_unchecked_mut() }.items.next();
        Poll::Ready(item)
    }
}
