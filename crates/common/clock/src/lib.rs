#![cfg_attr(test, deny(warnings))]

use mockall::automock;
use time::OffsetDateTime;

#[cfg(feature = "with-serde")]
pub mod serde;

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
