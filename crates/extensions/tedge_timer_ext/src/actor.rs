use crate::builder::TimerActorBuilder;
use crate::SetTimeout;
use crate::Timeout;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::pin::Pin;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ClientId;
use tedge_actors::ServiceMessageBox;
use tokio::time::sleep_until;
use tokio::time::Instant;

/// An actor that manages a set of timers
#[derive(Default)]
pub struct TimerActor {
    current_timer: Option<SleepHandle>,
    next_timers: BinaryHeap<TimerEntry>,
    next_timer_id: usize,
}

impl TimerActor {
    pub fn builder() -> TimerActorBuilder {
        TimerActorBuilder::default()
    }

    /// Push a new timeout request
    ///
    /// Update the current timer if this request is for an earlier deadline.
    /// Simply store the request for later otherwise.
    fn push(&mut self, timer_request: (ClientId, SetTimeout<TimerId>)) {
        let new_timer = self.new_entry(timer_request);

        // Check if the new timer is more urgent
        // Because of ownership, this test must be done in a function independent of the update.
        // Indeed one wants to drop the pending Sleep future but only when the new timer is more urgent.
        if self.current_timer_is_less_urgent_than(&new_timer) {
            // Cancel the former timer (by dropping the Sleep future)
            let maybe_former_timer = self.current_timer.take().map(|former| former.timer);

            // Set the new timer
            self.start_timer(new_timer);

            // Store for later the former timer, if any
            if let Some(former_timer) = maybe_former_timer {
                self.next_timers.push(former_timer);
            }
        } else {
            // The new timer will have to wait for its turn
            self.next_timers.push(new_timer);
        }
    }

    /// Create a new timer entry to which a fresh id has been assigned.
    fn new_entry(&mut self, timer_request: (ClientId, SetTimeout<TimerId>)) -> TimerEntry {
        self.next_timer_id += 1;

        let (client_id, timer) = timer_request;
        let deadline = Instant::now() + timer.duration;
        let timer_id = self.next_timer_id;
        let event_id = timer.event;

        TimerEntry {
            deadline,
            timer_id,
            client_id,
            event_id,
        }
    }

    /// Check if the new timer is more urgent that the current one, if any
    fn current_timer_is_less_urgent_than(&self, new_timer: &TimerEntry) -> bool {
        match &self.current_timer {
            None => true,
            Some(current) if new_timer > &current.timer => true,
            _ => false,
        }
    }

    /// Start to wait for the requested duration
    ///
    /// Don't wait for time to elapse!
    /// Simply store the Sleep future along the timer data.
    fn start_timer(&mut self, timer: TimerEntry) {
        assert!(self.current_timer.is_none());

        let sleep = sleep_until(timer.deadline);
        self.current_timer = Some(SleepHandle {
            timer,
            sleep: Box::pin(sleep),
        });
    }

    /// Start the next pending timer is any
    fn start_next_timer(&mut self) {
        if let Some(timer) = self.next_timers.pop() {
            self.start_timer(timer)
        }
    }
}

pub type TimerId = usize;

#[derive(Debug, Eq, PartialEq)]
struct TimerEntry {
    /// The deadline to raise this timer
    deadline: Instant,

    /// Internal id used to distinguish timers created by independent callers
    timer_id: TimerId,

    /// Identifier of the caller
    client_id: ClientId,

    /// Event id to be returned to the caller when the timer is raised
    event_id: TimerId,
}

/// Sort timer entries along the time line.
impl Ord for TimerEntry {
    /// Give priority to earlier deadline
    /// and to the smaller id when the deadlines are the same.
    fn cmp(&self, other: &Self) -> Ordering {
        match other.deadline.cmp(&self.deadline) {
            Ordering::Equal => self.timer_id.cmp(&other.timer_id),
            no_some_deadline => no_some_deadline,
        }
    }
}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A pending timer along a future that will awake when the requested time elapses
struct SleepHandle {
    timer: TimerEntry,
    sleep: Pin<Box<tokio::time::Sleep>>,
}

#[async_trait]
impl Actor for TimerActor {
    type MessageBox = ServiceMessageBox<SetTimeout<TimerId>, Timeout<TimerId>>;

    fn name(&self) -> &str {
        "Timer"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        loop {
            if let Some(current) = self.current_timer.take() {
                let time_elapsed = current.sleep;
                let current_timer = current.timer;
                // Wait either for a new request or the current timer to elapse
                tokio::select! {
                    () = time_elapsed => {
                        self.start_next_timer()
                    },
                    maybe_message = messages.recv() => {
                        // The current timer has to be restarted
                        self.start_timer(current_timer);

                        if let Some(timer_request) = maybe_message {
                            self.push(timer_request);
                        }
                    },
                }
            } else {
                // There is no pending timers
                // So simply wait for a timer request
                match messages.recv().await {
                    None => {
                        // Done: no more request, nor pending timer
                        break;
                    }
                    Some(timer_request) => self.push(timer_request),
                }
            }
        }

        Ok(())
    }
}
