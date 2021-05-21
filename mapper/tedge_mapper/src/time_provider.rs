use chrono::{DateTime, FixedOffset};

pub type Timestamp = DateTime<FixedOffset>;

pub trait TimeProvider {
    fn now(&self) -> Timestamp;
}

pub struct SystemTimeProvider;

impl TimeProvider for SystemTimeProvider {
    fn now(&self) -> Timestamp {
        thin_edge_json::measurement::current_timestamp()
    }
}

pub struct TestTimeProvider {
    pub now: Timestamp,
}

impl TimeProvider for TestTimeProvider {
    fn now(&self) -> Timestamp {
        self.now
    }
}
