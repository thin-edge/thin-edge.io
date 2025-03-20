use mockall::automock;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
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

#[automock]
pub trait MonotonicClock: Send + Sync + 'static {
    fn now(&self) -> TedgeInstant;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// A wrapper type to prevent accidental use of [Instant::elapsed]
///
/// Once a time is produced by [MonotonicClock], we don't want to use
/// [Instant::elapsed] as that will compare to [Instant::now] and not
/// [MontonicClock::now].
pub struct TedgeInstant(Instant);

pub struct SystemClock;

impl MonotonicClock for SystemClock {
    fn now(&self) -> TedgeInstant {
        TedgeInstant(Instant::now())
    }
}

impl TedgeInstant {
    pub fn elapsed(&self, clock: &(impl MonotonicClock + ?Sized)) -> Duration {
        clock.now().0 - self.0
    }
}

pub struct ManuallyDrivenClock {
    time: Mutex<Instant>,
    rx: Mutex<mpsc::Receiver<Instant>>,
}

impl ManuallyDrivenClock {
    pub fn new(time: Instant) -> (Self, mpsc::Sender<Instant>) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                time: Mutex::new(time),
                rx: Mutex::new(rx),
            },
            tx,
        )
    }
}

impl MonotonicClock for ManuallyDrivenClock {
    fn now(&self) -> TedgeInstant {
        let rx = self.rx.lock().unwrap();
        while let Ok(time) = rx.try_recv() {
            *self.time.lock().unwrap() = time;
        }
        TedgeInstant(*self.time.lock().unwrap())
    }
}
