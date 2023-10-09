use std::future::Future;
use tokio::pin;

pub async fn select<T>(one: impl Future<Output = T>, two: impl Future<Output = T>) -> T {
    pin!(one);
    pin!(two);
    futures::future::select(one, two).await.factor_first().0
}
