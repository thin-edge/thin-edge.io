use mockall::automock;
use time::OffsetDateTime;

pub type Timestamp = OffsetDateTime;

#[automock]
pub trait Clock: Sync + Send + 'static {
    fn now(&self) -> Timestamp;
}

#[derive(Clone)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> Timestamp {
        OffsetDateTime::now_utc()
    }
}
