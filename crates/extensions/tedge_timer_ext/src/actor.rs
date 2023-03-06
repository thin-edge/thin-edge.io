use crate::builder::TimerActorBuilder;
use crate::SetTimeout;
use crate::Timeout;
use async_trait::async_trait;
use std::any::Any;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt::Debug;
use std::pin::Pin;
use tedge_actors::Actor;
use tedge_actors::ClientId;
use tedge_actors::RuntimeError;
use tedge_actors::ServerMessageBox;
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
    fn push(&mut self, timer_request: (ClientId, SetTimeout<AnyPayload>)) {
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
    fn new_entry(&mut self, timer_request: (ClientId, SetTimeout<AnyPayload>)) -> TimerEntry {
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

/// Opaque type used by the timer actor to hold generic payloads provided by its peers
///
/// The conversions from `SetTimeout<T>` into `SetTimeout<AnyPayload>`,
/// as well as from `Timeout<AnyPayload>` into `Timeout<T>`,
/// are managed under the hood by the `TimerActor::builder()`.
pub type AnyPayload = Box<dyn Any + Send + Sync + 'static>;

#[derive(Debug)]
struct TimerEntry {
    /// The deadline to raise this timer
    deadline: Instant,

    /// Internal id used to distinguish timers created by independent callers
    timer_id: TimerId,

    /// Identifier of the caller
    client_id: ClientId,

    /// Event id to be returned to the caller when the timer is raised
    event_id: AnyPayload,
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

impl Eq for TimerEntry {}
impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline.eq(&other.deadline) && self.timer_id.eq(&other.timer_id)
    }
}

/// A pending timer along a future that will awake when the requested time elapses
struct SleepHandle {
    timer: TimerEntry,
    sleep: Pin<Box<tokio::time::Sleep>>,
}

#[async_trait]
impl Actor for TimerActor {
    type MessageBox = ServerMessageBox<SetTimeout<AnyPayload>, Timeout<AnyPayload>>;

    fn name(&self) -> &str {
        "Timer"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        loop {
            if let Some(current) = self.current_timer.take() {
                let time_elapsed = current.sleep;
                let current_timer = current.timer;
                // Wait either for a new request or the current timer to elapse
                tokio::select! {
                    () = time_elapsed => {
                        let caller = current_timer.client_id;
                        let response = Timeout {
                            event: current_timer.event_id
                        };
                        messages.send((caller, response)).await?;
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
