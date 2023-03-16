use crate::batch::Batch;
use crate::batch::BatchAdd;
use crate::batchable::Batchable;
use crate::config::BatchConfig;
use time::OffsetDateTime;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BatcherOutput<B> {
    Batch(Vec<B>),
    Timer(OffsetDateTime),
}

/// Provides the core implementation of the batching algorithm.
#[derive(Debug)]
pub struct Batcher<B: Batchable> {
    config: BatchConfig,
    batches: Vec<Batch<B>>,
}

impl<B: Batchable> Batcher<B> {
    /// Create a Batcher with the specified config.
    pub fn new(config: BatchConfig) -> Batcher<B> {
        Batcher {
            config,
            batches: vec![],
        }
    }

    pub(crate) fn event(
        &mut self,
        processing_time: OffsetDateTime,
        event: B,
    ) -> Vec<BatcherOutput<B>> {
        let event_time = event.event_time();

        if event_time < processing_time - self.config.delivery_jitter() {
            // Discard event because it is too old
            return vec![];
        }

        if event_time > processing_time + self.config.message_leap_limit() {
            // Discard event because it is too futuristic
            return vec![];
        }

        match self.find_target_batch(event_time) {
            None => {
                let new_batch = self.make_new_batch(event);
                let new_batch_end = new_batch.batch_end();
                self.batches.push(new_batch);
                self.output_for_batch_end(processing_time, new_batch_end)
            }
            Some(target_batch) => match target_batch.add(event) {
                BatchAdd::Added => vec![],
                BatchAdd::Duplicate => vec![],
                BatchAdd::Split(new_batch) => {
                    let split_batch_end = target_batch.batch_end();
                    self.batches.push(new_batch);
                    self.output_for_batch_end(processing_time, split_batch_end)
                }
            },
        }
    }

    fn output_for_batch_end(
        &mut self,
        processing_time: OffsetDateTime,
        batch_end: OffsetDateTime,
    ) -> Vec<BatcherOutput<B>> {
        let batch_timeout = batch_end + self.config.delivery_jitter();
        if processing_time < batch_timeout {
            vec![BatcherOutput::Timer(batch_timeout)]
        } else {
            self.time(processing_time)
                .into_iter()
                .map(BatcherOutput::Batch)
                .collect()
        }
    }

    pub(crate) fn time(&mut self, time: OffsetDateTime) -> Vec<Vec<B>> {
        let batches = std::mem::take(&mut self.batches);

        let (open_batches, closed_batches) = batches
            .into_iter()
            .partition(|batch| self.is_open(batch, time));

        self.batches = open_batches;

        closed_batches
            .into_iter()
            .map(|batch| batch.into_vec())
            .collect()
    }

    fn is_open(&self, batch: &Batch<B>, time: OffsetDateTime) -> bool {
        batch.batch_end() + self.config.delivery_jitter() > time
    }

    pub(crate) fn flush(&mut self) -> Vec<Vec<B>> {
        let mut batches = Vec::with_capacity(self.batches.len());

        while let Some(batch) = self.batches.pop() {
            batches.push(batch.into_vec())
        }

        batches
    }

    fn find_target_batch(&mut self, event_time: OffsetDateTime) -> Option<&mut Batch<B>> {
        self.batches
            .iter_mut()
            .find(|batch| batch.batch_start() <= event_time && event_time <= batch.batch_end())
    }

    fn make_new_batch(&self, event: B) -> Batch<B> {
        let event_time = event.event_time();
        let mut batch_start = event_time;
        let mut batch_end = batch_start + self.config.event_jitter();

        if let Some(previous_batch) = self.previous_batch(event_time) {
            batch_start = batch_start.max(previous_batch.batch_end())
        }
        if let Some(next_batch) = self.next_batch(event_time) {
            batch_end = batch_end.min(next_batch.batch_start())
        }

        Batch::new(batch_start, batch_end, event)
    }

    fn previous_batch(&self, event_time: OffsetDateTime) -> Option<&Batch<B>> {
        self.batches
            .iter()
            .filter(|batch| batch.batch_end() < event_time)
            .max_by(|batch1, batch2| batch1.batch_end().cmp(&batch2.batch_end()))
    }

    fn next_batch(&self, event_time: OffsetDateTime) -> Option<&Batch<B>> {
        self.batches
            .iter()
            .filter(|batch| batch.batch_start() > event_time)
            .min_by(|batch1, batch2| batch1.batch_start().cmp(&batch2.batch_start()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batchable::Batchable;
    use crate::config::BatchConfigBuilder;
    use std::collections::BTreeMap;
    use time::Duration;

    #[test]
    fn single_event_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);

        test.event(1, &event1);
        test.expect_batch(70, vec![event1]);

        test.run();
    }

    #[test]
    fn multi_event_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "b", 2);

        test.event(1, &event1);
        test.event(11, &event2);
        test.expect_batch(70, vec![event1, event2]);

        test.run();
    }

    #[test]
    // The same behavior as for `multi_event_batch` is expected
    // Since we just change how long we wait for an event
    fn multi_event_batch_with_long_delivery_jitter() {
        let mut test = BatcherTest::new(50, 50, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "b", 2);

        test.event(1, &event1);
        test.event(11, &event2);
        test.expect_batch(100, vec![event1, event2]);

        test.run();
    }

    #[test]
    fn multi_event_batch_with_long_delivery_jitter_and_delayed_message() {
        let mut test = BatcherTest::new(50, 50, 0);

        let event1 = test.create_event(5, "a", 2);
        let event2 = test.create_event(10, "b", 1);

        test.event(11, &event2);
        test.event(25, &event1); // late, but not too late

        test.expect_batch(60, vec![event1]);
        test.expect_batch(110, vec![event2]);

        test.run();
    }

    #[test]
    fn split_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "a", 2);

        test.event(1, &event1);
        test.event(11, &event2);
        test.expect_batch(25, vec![event1]); // why 25?
        test.expect_batch(70, vec![event2]);

        test.run();
    }

    #[test]
    fn allocate_to_earlier_split_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "a", 2);
        let event3 = test.create_event(2, "b", 3);

        test.event(1, &event1);
        test.event(11, &event2);
        test.event(12, &event3);
        test.expect_batch(25, vec![event1, event3]);
        test.expect_batch(70, vec![event2]);

        test.run();
    }

    #[test]
    fn allocate_to_later_split_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "a", 2);
        let event3 = test.create_event(9, "b", 3);

        test.event(1, &event1);
        test.event(11, &event2);
        test.event(12, &event3);
        test.expect_batch(25, vec![event1]);
        test.expect_batch(70, vec![event2, event3]);

        test.run();
    }

    #[test]
    fn flush_no_batches() {
        let mut test = BatcherTest::new(50, 20, 0);
        test.flush(100);
        test.run();
    }

    #[test]
    fn flush_one_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(10, "b", 2);

        test.event(1, &event1);
        test.event(11, &event2);
        test.flush(20);
        test.expect_batch(20, vec![event1, event2]);

        test.run();
    }

    #[test]
    fn flush_two_batches() {
        let mut test = BatcherTest::new(50, 20, 0);

        let event1 = test.create_event(0, "a", 1);
        let event2 = test.create_event(3, "b", 2);
        let event3 = test.create_event(10, "a", 3);

        test.event(1, &event1);
        test.event(4, &event2);
        test.event(11, &event3);
        test.flush(20);
        test.expect_batch(20, vec![event1, event2]);
        test.expect_batch(20, vec![event3]);

        test.run();
    }

    // The following tests are taken from the diagrams on the specification:
    // https://github.com/albinsuresh/thin-edge.io-specs/blob/main/src/telemetry-data/message-batching/message-batching.md

    #[test]
    fn simple_batching_with_batching_window() {
        let mut test = BatcherTest::new(50, 20, 0);

        let a = test.create_event(115, "a", 1);
        let b = test.create_event(120, "b", 2);
        let c = test.create_event(145, "c", 3);
        let d = test.create_event(160, "d", 4);
        let e = test.create_event(175, "e", 5);
        let f = test.create_event(215, "f", 6);
        let g = test.create_event(240, "g", 7);

        test.event(125, &b);
        test.event(135, &a); // order inversion
        test.event(150, &c);
        test.event(165, &d);
        test.event(189, &e);
        test.event(250, &g);
        test.event(260, &f); // too late
        test.expect_batch(140, vec![a]);
        test.expect_batch(190, vec![b, c, d]);
        test.expect_batch(245, vec![e]);
        test.expect_batch(310, vec![g]);

        test.run();
    }

    #[test]
    fn simple_batching_with_batching_timeout() {
        let mut test = BatcherTest::new(50, 20, 0);

        let a = test.create_event(120, "a", 1);
        let b = test.create_event(130, "b", 2);
        let c = test.create_event(145, "c", 3);
        let d = test.create_event(180, "d", 4);
        let e = test.create_event(190, "e", 5);

        test.event(130, &a);
        test.event(140, &b);
        test.event(150, &c);
        test.event(189, &d);
        test.event(210, &e);
        test.expect_batch(190, vec![a, b, c]);
        test.expect_batch(250, vec![d, e]);

        test.run();
    }

    #[test]
    fn batch_split_due_to_conflicting_measurements() {
        let mut test = BatcherTest::new(50, 20, 0);

        let a1 = test.create_event(120, "a", 1);
        let b1 = test.create_event(125, "b", 2);
        let a2 = test.create_event(140, "a", 3);
        let c1 = test.create_event(150, "c", 4);
        let a3 = test.create_event(170, "a", 5);

        test.event(125, &a1);
        test.event(140, &b1);
        test.event(150, &a2);
        test.event(170, &c1);
        test.event(180, &a3);
        test.expect_batch(150, vec![a1, b1]);
        test.expect_batch(180, vec![a2, c1]);
        test.expect_batch(190, vec![a3]);

        test.run();
    }

    #[test]
    fn receiving_older_already_batched_messages_after_starting_a_new_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let a = test.create_event(120, "a", 1);
        let b = test.create_event(130, "b", 2);
        let c = test.create_event(140, "c", 3);
        let d = test.create_event(190, "d", 4);
        let e = test.create_event(210, "e", 5);

        test.event(130, &a);
        test.event(140, &b);
        test.event(150, &c);
        test.event(160, &c);
        test.event(175, &c);
        test.event(210, &d);
        test.event(220, &c);
        test.event(230, &e);
        test.expect_batch(190, vec![a, b, c]);
        test.expect_batch(260, vec![d, e]);

        test.run();
    }

    #[test]
    fn receiving_older_unbatched_messages_after_starting_a_new_batch() {
        let mut test = BatcherTest::new(50, 20, 0);

        let a1 = test.create_event(120, "a", 1);
        let b1 = test.create_event(130, "b", 2);
        let c1 = test.create_event(140, "c", 3);
        let d1 = test.create_event(145, "d", 4);
        let a2 = test.create_event(180, "a", 5);
        let b2 = test.create_event(200, "b", 6);

        test.event(130, &a1);
        test.event(140, &b1);
        test.event(150, &c1);
        test.event(189, &a2);
        test.event(205, &b2);
        test.event(215, &d1);
        test.expect_batch(190, vec![a1, b1, c1]);
        test.expect_batch(250, vec![a2, b2]);

        test.run();
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct TestBatchEvent {
        event_time: OffsetDateTime,
        key: String,
        value: u64,
    }

    impl Batchable for TestBatchEvent {
        type Key = String;

        fn key(&self) -> Self::Key {
            self.key.clone()
        }

        fn event_time(&self) -> OffsetDateTime {
            self.event_time
        }
    }

    #[derive(Debug)]
    enum EventOrTimer {
        Event(TestBatchEvent),
        Timer(),
    }

    struct BatcherTest {
        start_time: OffsetDateTime,
        batcher: Batcher<TestBatchEvent>,
        inputs: BTreeMap<OffsetDateTime, EventOrTimer>,
        flush_time: Option<OffsetDateTime>,
        expected_batches: BTreeMap<OffsetDateTime, Vec<Vec<TestBatchEvent>>>,
    }

    impl BatcherTest {
        fn new(event_jitter: u32, delivery_jitter: u32, message_leap_limit: u32) -> BatcherTest {
            let batcher_config = BatchConfigBuilder::new()
                .event_jitter(event_jitter)
                .delivery_jitter(delivery_jitter)
                .message_leap_limit(message_leap_limit)
                .build();

            let start_time = OffsetDateTime::from_unix_timestamp(0).unwrap();
            let batcher = Batcher::new(batcher_config);

            BatcherTest {
                start_time,
                batcher,
                inputs: BTreeMap::new(),
                flush_time: None,
                expected_batches: BTreeMap::new(),
            }
        }

        fn create_event(&mut self, event_time: i64, key: &str, value: u64) -> TestBatchEvent {
            let event_time = self.create_instant(event_time);
            let key = key.into();
            TestBatchEvent {
                event_time,
                key,
                value,
            }
        }

        fn event(&mut self, processed_time: i64, event: &TestBatchEvent) {
            let processed_time = self.create_instant(processed_time);
            if let Some(_existing) = self
                .inputs
                .insert(processed_time, EventOrTimer::Event(event.clone()))
            {
                panic!("Two events with same processing time")
            }
        }

        fn flush(&mut self, flush_time: i64) {
            self.flush_time = Some(self.create_instant(flush_time));
        }

        fn expect_batch(&mut self, batch_close_time: i64, batch: Vec<TestBatchEvent>) {
            let batch_close_time = self.create_instant(batch_close_time);
            let batches_at_time = self.expected_batches.entry(batch_close_time).or_default();
            batches_at_time.push(batch);
        }

        fn run(mut self) {
            let mut actual_batches = BTreeMap::new();

            if let Some(flush_time) = self.flush_time {
                if !self.inputs.split_off(&flush_time).is_empty() {
                    panic!("Flush must be the last test action");
                }
            }

            while let Some((t, action)) = pop_first(&mut self.inputs) {
                match action {
                    EventOrTimer::Event(event) => {
                        let outputs = self.batcher.event(t, event);
                        self.handle_outputs(t, outputs, &mut actual_batches, self.flush_time);
                    }
                    EventOrTimer::Timer() => {
                        actual_batches.insert(t, self.batcher.time(t));
                    }
                };
            }

            if let Some(t) = self.flush_time {
                let batches = self.batcher.flush();
                if !batches.is_empty() {
                    actual_batches.insert(t, batches);
                }
            }

            verify(self.expected_batches, actual_batches);
        }

        fn handle_outputs(
            &mut self,
            t: OffsetDateTime,
            outputs: Vec<BatcherOutput<TestBatchEvent>>,
            all_batches: &mut BTreeMap<OffsetDateTime, Vec<Vec<TestBatchEvent>>>,
            flush_time: Option<OffsetDateTime>,
        ) {
            let mut batches = vec![];

            for output in outputs {
                match output {
                    BatcherOutput::Batch(batch) => batches.push(batch),
                    BatcherOutput::Timer(timer) => {
                        if timer <= t {
                            panic!(
                                "Batcher requested non-future timer. Input: {}, timer: {}",
                                t, timer
                            );
                        }
                        let add_timer = match flush_time {
                            None => true,
                            Some(flush_time) => timer < flush_time,
                        };
                        if add_timer {
                            if let Some(existing) = self.inputs.insert(timer, EventOrTimer::Timer())
                            {
                                panic!(
                                    "Timer at the same time as existing event/timer: {}: {:?}",
                                    timer, existing
                                );
                            }
                        }
                    }
                }
            }

            if !batches.is_empty() {
                all_batches.insert(t, batches);
            }
        }

        fn create_instant(&self, time: i64) -> OffsetDateTime {
            self.start_time + Duration::milliseconds(time)
        }
    }

    fn verify(
        expected_batches: BTreeMap<OffsetDateTime, Vec<Vec<TestBatchEvent>>>,
        mut actual_batches: BTreeMap<OffsetDateTime, Vec<Vec<TestBatchEvent>>>,
    ) {
        assert_eq!(
            actual_batches.keys().collect::<Vec<_>>(),
            expected_batches.keys().collect::<Vec<_>>()
        );

        for (time, timed_expected_batches) in expected_batches {
            let mut timed_actual_batches = actual_batches.remove(&time).unwrap();

            for timed_expected_batch in &timed_expected_batches {
                let found =
                    timed_actual_batches
                        .iter()
                        .enumerate()
                        .find(|(_index, timed_actual_batch)| {
                            match_batches(timed_actual_batch, timed_expected_batch)
                        });

                match found {
                    None => panic!(
                        "Failed to match batch @ {}: {:?}",
                        time, timed_actual_batches
                    ),
                    Some((index, _batch)) => timed_actual_batches.remove(index),
                };
            }
        }
    }

    fn match_batches(batch1: &[TestBatchEvent], batch2: &[TestBatchEvent]) -> bool {
        if batch1.len() != batch2.len() {
            return false;
        }

        for event in batch1 {
            if !batch2.contains(event) {
                return false;
            }
        }

        true
    }

    fn pop_first<K: Ord + Copy, V>(map: &mut BTreeMap<K, V>) -> Option<(K, V)> {
        let (&key, _value) = map.iter().next()?;
        map.remove_entry(&key)
    }
}
