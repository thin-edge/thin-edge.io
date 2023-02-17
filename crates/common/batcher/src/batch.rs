use crate::batchable::Batchable;
use std::collections::HashMap;
use std::iter::once;
use time::OffsetDateTime;

#[must_use]
#[derive(Debug)]
pub enum BatchAdd<B: Batchable> {
    Added,
    Duplicate,
    Split(Batch<B>),
}

#[derive(Debug)]
pub struct Batch<B: Batchable> {
    batch_start: OffsetDateTime,
    batch_end: OffsetDateTime,
    events: HashMap<B::Key, B>,
}

impl<B: Batchable> Batch<B> {
    pub fn new(batch_start: OffsetDateTime, batch_end: OffsetDateTime, event: B) -> Batch<B> {
        let mut events = HashMap::new();
        events.insert(event.key(), event);

        Batch {
            batch_start,
            batch_end,
            events,
        }
    }

    pub fn batch_start(&self) -> OffsetDateTime {
        self.batch_start
    }

    pub fn batch_end(&self) -> OffsetDateTime {
        self.batch_end
    }

    pub fn add(&mut self, event: B) -> BatchAdd<B> {
        let key = event.key();
        if let Some(existing_event) = self.events.get(&key) {
            let existing_event_time = existing_event.event_time();

            if event.event_time() == existing_event_time {
                return BatchAdd::Duplicate;
            }

            return BatchAdd::Split(self.split(existing_event_time, event));
        }

        self.events.insert(key, event);

        BatchAdd::Added
    }

    fn split(&mut self, existing_event_time: OffsetDateTime, event: B) -> Batch<B> {
        let split_point = midpoint(existing_event_time, event.event_time());

        let mut new_batch_events = HashMap::new();
        let new_batch_end = self.batch_end;

        let all_events = std::mem::take(&mut self.events);
        self.batch_end = split_point;

        // Go over all the events in this batch plus the new event and allocate them,
        // either the existing batch or the new batch.
        for event in all_events.into_values().chain(once(event)) {
            let event_time = event.event_time();

            if event_time < split_point {
                self.events.insert(event.key(), event);
            } else {
                new_batch_events.insert(event.key(), event);
            }
        }

        Batch {
            batch_start: split_point,
            batch_end: new_batch_end,
            events: new_batch_events,
        }
    }

    pub fn into_vec(self) -> Vec<B> {
        self.events.into_values().collect()
    }
}

fn midpoint(event_time1: OffsetDateTime, event_time2: OffsetDateTime) -> OffsetDateTime {
    let gap = event_time1 - event_time2;
    event_time2 + gap / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add() {
        let batch_start = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let batch_end = OffsetDateTime::from_unix_timestamp(100).unwrap();
        let event1 = TestBatchEvent::new(1, 40);
        let event2 = TestBatchEvent::new(2, 60);

        let mut batch = Batch::new(batch_start, batch_end, event1.clone());
        assert!(matches!(batch.add(event2.clone()), BatchAdd::Added));

        let result = batch.into_vec();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&event1));
        assert!(result.contains(&event2));
    }

    #[test]
    fn split() {
        let batch_start = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let batch_end = OffsetDateTime::from_unix_timestamp(100).unwrap();
        let event1 = TestBatchEvent::new(1, 40);
        let event2 = TestBatchEvent::new(1, 60);

        let mut batch1 = Batch::new(batch_start, batch_end, event1.clone());
        match batch1.add(event2.clone()) {
            BatchAdd::Split(batch2) => {
                let result1 = batch1.into_vec();
                assert_eq!(result1.len(), 1);
                assert!(result1.contains(&event1));

                let result2 = batch2.into_vec();
                assert_eq!(result2.len(), 1);
                assert!(result2.contains(&event2));
            }
            _ => panic!("Expected split"),
        }
    }

    #[test]
    fn duplicate() {
        let batch_start = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let batch_end = OffsetDateTime::from_unix_timestamp(100).unwrap();
        let event1 = TestBatchEvent::new(1, 40);
        let event2 = TestBatchEvent::new(1, 40);

        let mut batch = Batch::new(batch_start, batch_end, event1.clone());
        assert!(matches!(batch.add(event2), BatchAdd::Duplicate));

        let result = batch.into_vec();
        assert_eq!(result.len(), 1);
        assert!(result.contains(&event1));
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct TestBatchEvent {
        key: u64,
        event_time: OffsetDateTime,
    }

    impl TestBatchEvent {
        fn new(key: u64, event_time: i64) -> TestBatchEvent {
            let event_time = OffsetDateTime::from_unix_timestamp(event_time).unwrap();
            TestBatchEvent { key, event_time }
        }
    }

    impl Batchable for TestBatchEvent {
        type Key = u64;

        fn key(&self) -> Self::Key {
            self.key
        }

        fn event_time(&self) -> OffsetDateTime {
            self.event_time
        }
    }
}
