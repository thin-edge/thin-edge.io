use std::collections::HashMap;
use crate::message::Message;

trait Producer<M> {
    fn get(&self) -> futures::channel::mpsc::Receiver<M>;
}
trait Consumer<M> {
    fn set(&self) -> u64;
}

#[derive(Default)]
pub struct PubSubPeers<M> {
    producers: HashMap<String, Box<dyn Producer<M>>>,
    consumers: HashMap<String, Box<dyn Consumer<M>>>,
}

impl<M: Message> PubSubPeers<M> {


}

