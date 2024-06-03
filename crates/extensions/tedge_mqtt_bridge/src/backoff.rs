use std::ops::Deref;
use std::time::Duration;
use std::time::Instant;

use ::backoff::backoff::Backoff;
use ::backoff::exponential::ExponentialBackoff;
use ::backoff::Clock;

pub struct CustomBackoff<C> {
    eb: ExponentialBackoff<C>,
    last_failure: Option<Instant>,
    reset_timeout: Duration,
}

impl<C> CustomBackoff<C> {
    pub fn new(
        clock: C,
        initial_interval: Duration,
        max_interval: Duration,
        reset_timeout: Duration,
    ) -> Self {
        Self {
            eb: ExponentialBackoff {
                current_interval: initial_interval,
                initial_interval,
                randomization_factor: 0.5,
                multiplier: 2.0,
                max_interval,
                start_time: Instant::now(),
                max_elapsed_time: None,
                clock,
            },
            last_failure: None,
            reset_timeout,
        }
    }
}

impl<C: Clock> CustomBackoff<C> {
    #[must_use]
    pub fn backoff(&mut self) -> BackoffHandle<C> {
        let now = self.eb.clock.now();
        match self.last_failure {
            Some(time) if now - time < self.reset_timeout => (),
            Some(_) | None => {
                // TODO check we actually succeeded on the previous try too
                self.eb.reset()
            }
        };

        BackoffHandle {
            duration: self.eb.next_backoff().unwrap(),
            inner: self,
        }
    }

    pub async fn sleep(&mut self) {
        self.backoff().sleep().await
    }
}

pub struct BackoffHandle<'a, C: Clock> {
    inner: &'a mut CustomBackoff<C>,
    duration: Duration,
}

impl<'a, C: Clock> Deref for BackoffHandle<'a, C> {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.duration
    }
}

impl<'a, C: Clock> Drop for BackoffHandle<'a, C> {
    fn drop(&mut self) {
        self.inner.last_failure = Some(self.inner.eb.clock.now());
    }
}

impl<'a, C: Clock> BackoffHandle<'a, C> {
    pub async fn sleep(self) {
        tokio::time::sleep(self.duration).await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn backoff_is_30_seconds_when_requested_for_first_time() {
        let now = Instant::now();
        let clock = IterClock::new([now, now + Duration::from_secs(1)]);
        let mut backoff = deterministic_backoff(&clock);
        assert_eq!(backoff.backoff().duration, Duration::from_secs(30));
    }

    #[test]
    fn backoff_is_60_seconds_when_requested_for_second_time() {
        let now = Instant::now();
        let clock = IterClock::new([now, now + Duration::from_secs(1)]);
        let mut backoff = deterministic_backoff(&clock);
        let _ = backoff.backoff();
        clock.tick();
        assert_eq!(backoff.backoff().duration, Duration::from_secs(60));
    }

    #[test]
    fn backoff_is_a_maximum_of_ten_minutes() {
        let now = Instant::now();
        let clock = IterClock::new([now]);
        let mut backoff = deterministic_backoff(&clock);
        for _ in 0..1000 {
            clock.add(backoff.backoff().duration);
        }
        assert_eq!(backoff.backoff().duration, Duration::from_secs(600));
        assert_eq!(backoff.backoff().duration, Duration::from_secs(600));
    }

    #[test]
    fn backoff_is_30_seconds_after_timeout() {
        let now = Instant::now();
        let clock = IterClock::new([now, now + Duration::from_secs(10 * 60)]);
        let mut backoff = deterministic_backoff(&clock);
        let _ = backoff.backoff();
        clock.tick();
        assert_eq!(backoff.backoff().duration, Duration::from_secs(30));
    }

    /// Creates a [CustomBackoff] with randomization disabled for deterministic testing
    fn deterministic_backoff(clock: &IterClock) -> CustomBackoff<&IterClock> {
        let mut backoff = CustomBackoff::new(
            clock,
            Duration::from_secs(30),
            Duration::from_secs(10 * 60),
            Duration::from_secs(5 * 60),
        );
        backoff.eb.randomization_factor = 0.0;
        backoff
    }

    struct IterClock(RefCell<VecDeque<Instant>>);

    impl IterClock {
        pub fn new(durations: impl Into<VecDeque<Instant>>) -> Self {
            Self(RefCell::new(durations.into()))
        }
        pub fn tick(&self) {
            self.0.borrow_mut().pop_front();
        }
        pub fn add(&self, duration: Duration) {
            *self.0.borrow_mut().front_mut().unwrap() += duration;
        }
    }

    impl Clock for &IterClock {
        fn now(&self) -> Instant {
            self.0
                .borrow()
                .front()
                .copied()
                .expect("IterClock has exhausted its values")
        }
    }
}
