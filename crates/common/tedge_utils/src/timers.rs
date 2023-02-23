use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures::Future;
use futures::StreamExt;
use tokio::time::sleep;
use tokio::time::Instant;

pub type TimerFuture<T> = Pin<Box<dyn Future<Output = T> + Send + Sync>>;

#[derive(Debug, Default)]
pub struct Timers<K, V> {
    unfinished_child_op_status_map: HashMap<K, (V, Instant)>,
    unfinished_child_op_timers: FuturesUnordered<TimerFuture<(K, Instant)>>,
}

impl<K, V> Timers<K, V>
where
    K: Debug + Eq + Hash + Clone + Send + Sync + 'static,
    V: 'static,
{
    pub fn new() -> Self {
        Timers {
            unfinished_child_op_status_map: HashMap::new(),
            unfinished_child_op_timers: FuturesUnordered::new(),
        }
    }

    /// Schedule a timer `Future` which will timeout after the given duration,
    /// returning the provided `key` and `value` which can be retrieved
    /// with a call to the `next_timed_out_entry()` method after that `duration`.
    ///
    /// To "reset" an already scheduled timer,
    /// add a new one with the same key which will "logically" replace the former timer.
    pub fn start_timer(&mut self, key: K, value: V, duration: Duration) {
        let timer_start = Instant::now();
        let op_timer = Box::pin(Self::timer(key.clone(), duration, timer_start));
        self.unfinished_child_op_timers.push(op_timer);
        self.unfinished_child_op_status_map
            .insert(key, (value, timer_start));
    }

    pub fn stop_timer(&mut self, key: K) -> Option<V> {
        self.unfinished_child_op_status_map
            .remove(&key)
            .map(|tuple| tuple.0)
    }

    pub fn current_value(&self, key: &K) -> Option<&V> {
        self.unfinished_child_op_status_map
            .get(key)
            .map(|value| &value.0)
    }

    /// Return the `key` and `value` associated with the oldest timer that timed-out.
    /// Returns `None` when no timers are scheduled.
    /// The timers which got replaced by newer ones with the same `key` are ignored.
    ///
    /// Finding the next timed-out entry is not trivial because
    /// FuturesUnordered doesn't let you cancel/reset a TimerFuture that was already added to it.
    /// A timer once scheduled will definitely time-out eventually,
    /// even if the entry that started that timer completes successfully.
    /// So, when a timer times-out, we check if it was the latest timer started for that key.
    /// Else we discard that timer and continue until the latest timer times-out.
    pub async fn next_timed_out_entry(&mut self) -> Option<(K, V)> {
        while let Some((key, timer_update_instant)) = self.unfinished_child_op_timers.next().await {
            if let Some((_, last_timer_update_instant)) =
                self.unfinished_child_op_status_map.get(&key)
            {
                if timer_update_instant == *last_timer_update_instant {
                    let (value, _) = self
                        .unfinished_child_op_status_map
                        .remove(&key)
                        .expect("Infallible");
                    return Some((key, value));
                }
            }
        }
        None
    }

    pub async fn timer(key: K, duration: Duration, timer_start: Instant) -> (K, Instant) {
        sleep(duration).await;
        (key, timer_start)
    }
}
