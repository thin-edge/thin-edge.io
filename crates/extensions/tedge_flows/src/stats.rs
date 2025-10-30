use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::time::Duration;
use std::time::Instant;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

#[derive(Default)]
pub struct Counter {
    from_start: HashMap<Dimension, Stats>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub enum Dimension {
    Runtime,
    Flow(String),
    OnMessage(String),
    OnInterval(String),
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
    pub fn runtime_on_message_start(&mut self) -> Instant {
        self.add(Dimension::Runtime, Sample::MessageIn);
        Instant::now()
    }

    pub fn runtime_on_message_done(&mut self, started_at: Instant) {
        self.add(
            Dimension::Runtime,
            Sample::ProcessingTime(started_at.elapsed()),
        );
    }

    pub fn flow_on_message_start(&mut self, flow_id: &str) -> Instant {
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::MessageIn);
        Instant::now()
    }

    pub fn flow_on_message_done(&mut self, flow_id: &str, started_at: Instant, count: usize) {
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

    pub fn flow_on_message_failed(&mut self, flow_id: &str) {
        self.add(Dimension::Runtime, Sample::ErrorRaised);
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::ErrorRaised);
    }

    pub fn flow_on_interval_start(&mut self, _flow_id: &str) -> Instant {
        Instant::now()
    }

    pub fn flow_on_interval_done(&mut self, flow_id: &str, _started_at: Instant, count: usize) {
        self.add(Dimension::Runtime, Sample::MessageOut(count));
        self.add(
            Dimension::Flow(flow_id.to_owned()),
            Sample::MessageOut(count),
        );
    }

    pub fn flow_on_interval_failed(&mut self, flow_id: &str) {
        self.add(Dimension::Runtime, Sample::ErrorRaised);
        self.add(Dimension::Flow(flow_id.to_owned()), Sample::ErrorRaised);
    }

    pub fn flow_step_start(&mut self, js: &str, f: &str) -> Instant {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim, Sample::MessageIn);
        }
        Instant::now()
    }

    pub fn flow_step_done(&mut self, js: &str, f: &str, started_at: Instant, count: usize) {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim.clone(), Sample::MessageOut(count));
            self.add(dim, Sample::ProcessingTime(started_at.elapsed()));
        }
    }

    pub fn flow_step_failed(&mut self, js: &str, f: &str) {
        if let Some(dim) = Dimension::function_call(js, f) {
            self.add(dim.clone(), Sample::ErrorRaised);
        }
    }

    fn add(&mut self, dim: Dimension, sample: Sample) {
        self.from_start.entry(dim).or_default().add(sample);
    }

    pub fn dump_processing_stats<P: StatsPublisher>(&self, publisher: &P) -> Vec<P::Record> {
        tracing::info!(target: "flows", "Processing statistics:");
        self.from_start
            .iter()
            .filter_map(|(dim, stats)| stats.dump_statistics(dim, publisher))
            .collect()
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

    pub fn dump_statistics<P: StatsPublisher>(
        &self,
        dim: &Dimension,
        publisher: &P,
    ) -> Option<P::Record> {
        let stats = match self.processing_time.as_ref() {
            None => serde_json::json!({
                "input": self.messages_in,
                "output": self.messages_out,
                "error": self.error_raised,
            }),
            Some(duration_stats) => serde_json::json!({
                "input": self.messages_in,
                "output": self.messages_out,
                "error": self.error_raised,
                "cpu-min": format!("{:?}", duration_stats.min),
                "cpu-max": format!("{:?}", duration_stats.max),
            }),
        };

        publisher.publish(dim, stats)
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
            Dimension::OnMessage(js) => write!(f, "onMessage step {js}"),
            Dimension::OnInterval(js) => write!(f, "onInterval step {js}"),
        }
    }
}

impl Dimension {
    pub fn function_call(js: &str, f: &str) -> Option<Dimension> {
        match f {
            "onMessage" => Some(Dimension::OnMessage(js.to_owned())),
            "onInterval" => Some(Dimension::OnInterval(js.to_owned())),
            _ => None,
        }
    }
}

pub trait StatsPublisher {
    type Record;

    fn publish(&self, dim: &Dimension, stats: serde_json::Value) -> Option<Self::Record>;
}

pub struct TracingStatsPublisher;

impl StatsPublisher for TracingStatsPublisher {
    type Record = ();

    fn publish(&self, dim: &Dimension, stats: Value) -> Option<()> {
        tracing::info!(target: "flows", "  - {dim}");
        if let Some(stats) = stats.as_object() {
            for (k, v) in stats {
                tracing::info!(target: "flows", "    - {k}: {v}");
            }
        }
        None
    }
}

pub struct MqttStatsPublisher {
    pub topic_prefix: String,
}

impl StatsPublisher for MqttStatsPublisher {
    type Record = MqttMessage;

    fn publish(&self, dim: &Dimension, stats: Value) -> Option<Self::Record> {
        match dim {
            Dimension::Flow(path) | Dimension::OnMessage(path) => {
                self.topic_for(path).map(|topic| {
                    let payload = stats.to_string();
                    MqttMessage::new(&topic, payload)
                })
            }

            Dimension::Runtime => self.topic_for("runtime").map(|topic| {
                let payload = stats.to_string();
                MqttMessage::new(&topic, payload)
            }),

            _ => None,
        }
    }
}

impl MqttStatsPublisher {
    pub fn topic_for(&self, path: &str) -> Option<Topic> {
        let name = path.split('/').next_back().unwrap_or(path);
        let topic = format!("{}/{}", self.topic_prefix, name);
        Topic::new(&topic).ok()
    }
}
