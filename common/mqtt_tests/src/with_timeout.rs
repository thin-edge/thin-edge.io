use core::future::Future;
use tokio::time::timeout;
use tokio::time::{Duration, Timeout};

pub trait WithTimeout<T>
where
    T: Future,
{
    fn with_timeout(self, duration: Duration) -> Timeout<T>;
}

impl<F> WithTimeout<F> for F
where
    F: Future,
{
    fn with_timeout(self, duration: Duration) -> Timeout<F> {
        timeout(duration, self)
    }
}

pub trait Maybe<T> {
    fn expect_or(self, msg: &str) -> T;
}

impl<T, E> Maybe<T> for Result<Option<T>, E> {
    fn expect_or(self, msg: &str) -> T {
        match self {
            Ok(Some(x)) => x,
            Err(_) | Ok(None) => panic!("{}", msg),
        }
    }
}

pub async fn recv_with_timeout<T>(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<T>,
) -> Option<T> {
    match receiver.recv().with_timeout(Duration::from_secs(1)).await {
        Err(_) => None,
        Ok(msg) => msg,
    }
}
