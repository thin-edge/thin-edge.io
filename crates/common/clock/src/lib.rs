use chrono::{DateTime, FixedOffset, Local};
use mockall::automock;

pub type Timestamp = DateTime<FixedOffset>;

#[automock]
pub trait Clock: Sync + Send + 'static {
    fn now(&self) -> Timestamp;
}

#[derive(Clone)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> Timestamp {
        let local_time_now = Local::now();
        local_time_now.with_timezone(local_time_now.offset())
    }
}
