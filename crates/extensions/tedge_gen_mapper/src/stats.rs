use std::collections::HashMap;
use std::fmt::Display;
use std::time::Duration;
use std::time::Instant;

#[derive(Default)]
pub struct Counter {
    from_start: HashMap<Dimension, Stats>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub enum Dimension {
    Runtime,
    Flow(String),
    Process(String),
    Tick(String),
    Update(String),
}

pub enum Sample {
    MessageIn,
    MessageOut(usize),
    ErrorRaised,
    ProcessingTime(Duration),
}

#[derive(Default)]
pub struct Stats {
    messages_in: usize,
    messages_out: usize,
    error_raised: usize,
    processing_time: Option<DurationStats>,
}

pub struct DurationStats {
    min: Duration,
    max: Duration,
}

impl Counter {
    pub fn runtime_process_start(&mut self) -> Instant {
        self.add(Dimension::Runtime, Sample::MessageIn);
        Instant::now()
    }

    pub fn runtime_process_done(&mut self, started_at: Instant) {
        self.add(
            Dimension::Runtime,
            Sample::ProcessingTime(started_at.elapsed()),
        );
    }

    pub fn flow_process_start(&mut self, flow_id: &str) -> Instant {
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::MessageIn);
        Instant::now()
    }

    pub fn flow_process_done(&mut self, flow_id: &str, started_at: Instant, count: usize) {
        self.add(Dimension::Runtime, Sample::MessageOut(count));
        self.add(
            Dimension::Flow(flow_id.to_owned()),
            Sample::MessageOut(count),
        );
        self.add(
            Dimension::Flow(flow_id.to_owned()),
            Sample::ProcessingTime(started_at.elapsed()),
        );
    }

    pub fn flow_process_failed(&mut self, flow_id: &str) {
        self.add(Dimension::Runtime, Sample::ErrorRaised);
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::ErrorRaised);
    }

    pub fn flow_tick_start(&mut self, _flow_id: &str) -> Instant {
        Instant::now()
    }

    pub fn flow_tick_done(&mut self, flow_id: &str, _started_at: Instant, count: usize) {
        self.add(Dimension::Runtime, Sample::MessageOut(count));
        self.add(
            Dimension::Flow(flow_id.to_owned()),
            Sample::MessageOut(count),
        );
    }

    pub fn flow_tick_failed(&mut self, flow_id: &str) {
        self.add(Dimension::Runtime, Sample::ErrorRaised);
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::ErrorRaised);
    }

    pub fn filter_start(&mut self, js: &str, f: &str) -> Instant {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim, Sample::MessageIn);
        }
        Instant::now()
    }

    pub fn filter_done(&mut self, js: &str, f: &str, started_at: Instant, count: usize) {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim.clone(), Sample::MessageOut(count));
            self.add(dim, Sample::ProcessingTime(started_at.elapsed()));
        }
    }

    pub fn filter_failed(&mut self, js: &str, f: &str) {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim.clone(), Sample::ErrorRaised);
        }
    }

    fn add(&mut self, dim: Dimension, sample: Sample) {
        self.from_start.entry(dim).or_default().add(sample);
    }

    pub fn dump_processing_stats(&self) {
        tracing::info!(target: "gen-mapper", "Processing statistics:");
        for (dim, stats) in &self.from_start {
            stats.dump_statistics(dim)
        }
    }
}

impl Stats {
    pub fn add(&mut self, sample: Sample) {
        match sample {
            Sample::MessageIn => {
                self.messages_in += 1;
            }
            Sample::MessageOut(count) => {
                self.messages_out += count;
            }
            Sample::ErrorRaised => {
                self.error_raised += 1;
            }
            Sample::ProcessingTime(t) => match self.processing_time.as_mut() {
                None => self.processing_time = Some(DurationStats::new(t)),
                Some(stats) => stats.add(t),
            },
        }
    }

    pub fn dump_statistics(&self, dim: &Dimension) {
        tracing::info!(target: "gen-mapper", "    - {dim}");
        tracing::info!(target: "gen-mapper", "         - input count: {}", self.messages_in);
        tracing::info!(target: "gen-mapper", "         - output count: {}", self.messages_out);
        tracing::info!(target: "gen-mapper", "         - error count: {}", self.error_raised);
        if let Some(duration_stats) = &self.processing_time {
            tracing::info!(target: "gen-mapper", "         - min processing time: {:?}", duration_stats.min);
            tracing::info!(target: "gen-mapper", "         - max processing time: {:?}", duration_stats.max);
        }
    }
}

impl DurationStats {
    pub fn new(duration: Duration) -> Self {
        DurationStats {
            min: duration,
            max: duration,
        }
    }

    pub fn add(&mut self, duration: Duration) {
        if duration < self.min {
            self.min = duration;
        }
        if self.max < duration {
            self.max = duration;
        }
    }
}

impl Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dimension::Runtime => write!(f, "runtime"),
            Dimension::Flow(toml) => write!(f, "flow {toml}"),
            Dimension::Process(js) => write!(f, "process filter {js}"),
            Dimension::Tick(js) => write!(f, "tick filter {js}"),
            Dimension::Update(js) => write!(f, "update_config filter {js}"),
        }
    }
}

impl Dimension {
    pub fn function_call(js: &str, f: &str) -> Option<Dimension> {
        match f {
            "process" => Some(Dimension::Process(js.to_owned())),
            "tick" => Some(Dimension::Tick(js.to_owned())),
            "update_config" => Some(Dimension::Update(js.to_owned())),
            _ => None,
        }
    }
}
