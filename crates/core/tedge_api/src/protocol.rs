use async_trait::async_trait;

pub trait Producer<M> {
    //fn add_consumer(&mut self, consumer: Consumer<M>);
}

#[async_trait]
pub trait Consumer<M> {
    //async fn consume(&self, message: M);
}

pub trait Requester<Req, Res> {}

pub trait Responder<Req, Res> {}
