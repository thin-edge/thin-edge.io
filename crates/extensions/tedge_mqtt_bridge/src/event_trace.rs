//! Records a bounded history of what each bridge half did, for inspection after a failure
//!
//! A disabled [EventTrace] holds no buffer, so recording costs one branch on an `Option`
//! and allocates nothing. Nothing is formatted until [EventTrace::dump] is called, which
//! keeps the recording path cheap enough not to disturb the timing of whatever is being
//! investigated.

use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

/// A bounded record of bridge events, shared between the two halves of one bridge
///
/// [EventTrace::default] records nothing, which is what the mappers use
#[derive(Clone, Default)]
pub struct EventTrace(Option<Arc<Shared>>);

struct Shared {
    started_at: Instant,
    capacity: usize,
    records: Mutex<VecDeque<Record>>,
}

/// One thing that happened in a bridge half
struct Record {
    at: Instant,
    /// Which half recorded this, `"local"` or `"cloud"`
    half: &'static str,
    /// What happened, a short fixed label
    what: &'static str,
    /// The packet id involved, or 0 where there is none
    pkid: u16,
    /// How many messages were awaiting acknowledgement at the time
    waiting: usize,
}

impl EventTrace {
    /// Creates a trace keeping the most recent `capacity` events
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Some(Arc::new(Shared {
            started_at: Instant::now(),
            capacity,
            records: Mutex::new(VecDeque::with_capacity(capacity)),
        })))
    }

    /// Records one event, doing nothing when the trace is disabled
    pub(crate) fn record(&self, half: &'static str, what: &'static str, pkid: u16, waiting: usize) {
        let Some(shared) = &self.0 else {
            return;
        };
        let record = Record {
            at: Instant::now(),
            half,
            what,
            pkid,
            waiting,
        };
        let mut records = shared.records.lock().unwrap();
        if records.len() == shared.capacity {
            records.pop_front();
        }
        records.push_back(record);
    }

    /// Returns the recorded history, oldest first
    ///
    /// Consecutive identical events are collapsed so a long run of forwarding does not
    /// bury the interesting part
    pub fn dump(&self) -> String {
        let Some(shared) = &self.0 else {
            return "bridge event trace is disabled\n".into();
        };
        let records = shared.records.lock().unwrap();
        if records.is_empty() {
            return "bridge event trace is empty\n".into();
        }

        let mut out = format!("bridge event trace ({} records):\n", records.len());
        let mut run: Option<(&Record, usize)> = None;
        for record in records.iter() {
            match run {
                Some((previous, count))
                    if previous.half == record.half
                        && previous.what == record.what
                        && previous.pkid == record.pkid =>
                {
                    run = Some((previous, count + 1));
                }
                Some((previous, count)) => {
                    write_record(&mut out, previous, count, shared.started_at);
                    run = Some((record, 1));
                }
                None => run = Some((record, 1)),
            }
        }
        if let Some((last, count)) = run {
            write_record(&mut out, last, count, shared.started_at);
        }
        out
    }
}

fn write_record(out: &mut String, record: &Record, count: usize, started_at: Instant) {
    let millis = record.at.duration_since(started_at).as_secs_f64() * 1000.0;
    let repeat = if count > 1 {
        format!(" x{count}")
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "{millis:9.3}ms [{half}] {what} pkid={pkid} waiting={waiting}{repeat}",
        half = record.half,
        what = record.what,
        pkid = record.pkid,
        waiting = record.waiting,
    );
}
