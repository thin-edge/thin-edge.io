use async_trait::async_trait;

#[async_trait]
pub trait StreamInput<T>: Sized
where
    T: Send,
{
    async fn next(&mut self) -> Option<T>;

    fn filter<P>(self, predicate: P) -> Filter<Self, P>
    where
        P: Fn(&T) -> bool,
    {
        Filter {
            items: self,
            predicate,
        }
    }
}

#[async_trait]
impl<T> StreamInput<T> for async_broadcast::Receiver<T>
where
    T: Clone + Send,
{
    async fn next(&mut self) -> Option<T> {
        self.recv().await.ok()
    }
}

#[async_trait]
impl<T> StreamInput<T> for Vec<T>
where
    T: Clone + Send,
{
    async fn next(&mut self) -> Option<T> {
        self.pop()
    }
}

pub struct Filter<S, P>
where
    S: Sized,
    P: Sized,
{
    items: S,
    predicate: P,
}

#[async_trait]
impl<S, T, P> StreamInput<T> for Filter<S, P>
where
    T: Send,
    S: StreamInput<T> + Send,
    P: Fn(&T) -> bool + Send,
{
    async fn next(&mut self) -> Option<T> {
        while let Some(item) = self.items.next().await {
            if (self.predicate)(&item) {
                return Some(item);
            }
        }
        None
    }
}

#[async_trait]
pub trait StreamOutput<T>
where
    T: Send,
{
    async fn push(&mut self, item: T) -> Result<(), ()>;

    fn done(self);
}

#[async_trait]
impl<T> StreamOutput<T> for async_channel::Sender<T>
where
    T: Clone + Send,
{
    async fn push(&mut self, item: T) -> Result<(), ()> {
        self.send(item)
            .await
            .map_err(|async_channel::SendError(_)| ())
    }

    fn done(self) {
        let _ = self.close();
    }
}

pub struct StreamRecorder<T> {
    sender: Option<async_channel::Sender<T>>,
    receiver: async_channel::Receiver<T>,
}

impl<T> StreamRecorder<T>
where
    T: Clone + Send,
{
    pub fn new() -> StreamRecorder<T> {
        let (sender, receiver) = async_channel::unbounded();
        StreamRecorder {
            sender: Some(sender),
            receiver,
        }
    }

    pub fn collector_stream(&mut self) -> impl StreamOutput<T> {
        self.sender.take().expect("already taken")
    }

    pub async fn collected(self) -> Vec<T> {
        let mut items = vec![];
        loop {
            match self.receiver.recv().await.ok() {
                Some(item) => items.push(item),
                None => return items,
            }
        }
    }
}
