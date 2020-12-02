use core::fmt;
use tokio::stream::Stream;
use std::task::Context;
use tokio::macros::support::{Pin, Poll};

pub struct Client {
    pub name: String,
}

impl Client {
    pub async fn connect(name: &str) -> Result<Client, Error> {
        unimplemented!();
    }

    pub async fn publish(&self, message: Message) -> Result<(), Error> {
        unimplemented!();
    }

    //pub async fn subscribe<S : Stream<Item = Message>>(&self, topic: Topic) -> Result<S, Error> {
    pub async fn subscribe(&self, topic: &Topic) -> Result<MessageStream, Error> {
        unimplemented!();
    }

    pub async fn disconnect(self) -> Result<(), Error> {
        unimplemented!();
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Topic {
    pub name: String,
}

impl Topic {
    pub fn new(name: &str) -> Topic {
        let name = String::from(name);
        Topic {name}
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub topic: Topic,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new<B>(topic: &Topic, payload: B) -> Message
        where
            B: Into<Vec<u8>>,
    {
        Message{
            topic: topic.clone(),
            payload: payload.into(),
        }
    }
}

pub struct MessageStream {

}

impl Stream for MessageStream {
    type Item = Message;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unimplemented!()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Error {

}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
