use crate::config::FlowConfig;
use crate::flow::DateTime;
use crate::flow::Flow;
use crate::flow::FlowError;
use crate::flow::FlowInput;
use crate::flow::Message;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use fjall::Keyspace;
use fjall::PartitionCreateOptions;
use fjall::Slice;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tokio::task::spawn_blocking;
use tokio::time::Instant;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct MessageProcessor {
    pub config_dir: Utf8PathBuf,
    pub flows: HashMap<String, Flow>,
    pub(super) js_runtime: JsRuntime,
    pub stats: Counter,
    pub database: MeaDB,
}

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    Fjall(#[from] fjall::Error),
}

pub type MeaDB = MeaDb<DateTime, Message>;

impl MessageProcessor {
    pub fn flow_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Utf8Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load(config_dir).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    fn db_path() -> Utf8PathBuf {
        "/etc/tedge/tedge-flows.db".into()
    }

    pub async fn try_new_single_flow(
        config_dir: impl AsRef<Utf8Path>,
        flow: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let flow = flow.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_flow(&flow).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    pub async fn try_new_single_step_flow(
        config_dir: impl AsRef<Utf8Path>,
        script: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_script(&script).await;
        let flows = flow_specs.compile(&mut js_runtime, config_dir).await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for flow in self.flows.values() {
            topics.add_all(flow.topics())
        }
        topics
    }

    fn deadlines(&self) -> impl Iterator<Item = tokio::time::Instant> + '_ {
        self.flows
            .values()
            .flat_map(|flow| &flow.steps)
            .filter_map(|step| step.script.next_execution)
    }

    /// Get the next deadline for interval execution across all scripts
    /// Returns None if no scripts have intervals configured
    pub fn next_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().min()
    }

    /// Get the last deadline for interval execution across all scripts Returns
    /// None if no scripts have intervals configured
    ///
    /// This is intended for `tedge flows test` to ensure it processes all
    /// intervals
    pub fn last_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().max()
    }

    pub async fn on_message(
        &mut self,
        timestamp: DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FlowError>)> {
        let started_at = self.stats.runtime_on_message_start();

        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
                .await;
            if flow_output.is_err() {
                self.stats.flow_on_message_failed(flow_id);
            }
            out_messages.push((flow_id.clone(), flow_output));
        }

        self.stats.runtime_on_message_done(started_at);
        out_messages
    }

    pub async fn on_interval(
        &mut self,
        timestamp: DateTime,
        now: Instant,
    ) -> Vec<(String, Result<Vec<Message>, FlowError>)> {
        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .on_interval(&self.js_runtime, &mut self.stats, timestamp, now)
                .await;
            if flow_output.is_err() {
                self.stats.flow_on_interval_failed(flow_id);
            }
            out_messages.push((flow_id.clone(), flow_output));
        }
        out_messages
    }

    pub async fn process(
        &mut self,
        timestamp: DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FlowError>)> {
        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
                .await;
            out_messages.push((flow_id.clone(), flow_output));
        }
        out_messages
    }

    pub async fn tick(
        &mut self,
        timestamp: DateTime,
        now: Instant,
    ) -> Vec<(String, Result<Vec<Message>, FlowError>)> {
        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .on_interval(&self.js_runtime, &mut self.stats, timestamp, now)
                .await;
            out_messages.push((flow_id.clone(), flow_output));
        }
        out_messages
    }

    pub async fn drain_db(
        &mut self,
        timestamp: DateTime,
    ) -> Vec<(String, Result<Vec<(DateTime, Message)>, DatabaseError>)> {
        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter() {
            if let FlowInput::MeaDB {
                series: input_series,
                frequency: input_frequency,
                max_age: input_span,
            } = &flow.input
            {
                if timestamp.tick_now(*input_frequency) {
                    let cutoff_time = timestamp.sub_duration(*input_span);
                    let drained_messages = self
                        .database
                        .drain_older_than(cutoff_time, input_series)
                        .await
                        .map_err(DatabaseError::from);
                    out_messages.push((flow_id.to_owned(), drained_messages));
                }
            }
        }
        out_messages
    }

    pub async fn dump_processing_stats(&self) {
        self.stats.dump_processing_stats();
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn reload_script(&mut self, path: Utf8PathBuf) {
        for flow in self.flows.values_mut() {
            for step in &mut flow.steps {
                if step.script.path() == path {
                    match self.js_runtime.load_script(&mut step.script).await {
                        Ok(()) => {
                            step.script.init_next_execution();
                            info!(target: "flows", "Reloaded flow script {path}");
                        }
                        Err(e) => {
                            error!(target: "flows", "Failed to reload flow script {path}: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }

    pub async fn remove_script(&mut self, path: Utf8PathBuf) {
        for (flow_id, flow) in self.flows.iter() {
            for step in flow.steps.iter() {
                if step.script.path() == path {
                    warn!(target: "flows", "Removing a script used by a flow {flow_id}: {path}");
                    return;
                }
            }
        }
    }

    pub async fn load_flow(&mut self, flow_id: String, path: Utf8PathBuf) -> bool {
        let Ok(source) = tokio::fs::read_to_string(&path).await else {
            self.remove_flow(path).await;
            return false;
        };
        let config: FlowConfig = match toml::from_str(&source) {
            Ok(config) => config,
            Err(e) => {
                error!(target: "flows", "Failed to parse toml for flow {path}: {e}");
                return false;
            }
        };
        match config
            .compile(&mut self.js_runtime, &self.config_dir, path.clone())
            .await
        {
            Ok(flow) => {
                self.flows.insert(flow_id, flow);
                true
            }
            Err(e) => {
                error!(target: "flows", "Failed to compile flow {path}: {e}");
                false
            }
        }
    }

    pub async fn add_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if !self.flows.contains_key(&flow_id) && self.load_flow(flow_id, path.clone()).await {
            info!(target: "flows", "Loaded new flow {path}");
        }
    }

    pub async fn reload_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        if self.flows.contains_key(&flow_id) && self.load_flow(flow_id, path.clone()).await {
            info!(target: "flows", "Reloaded updated flow {path}");
        }
    }

    pub async fn remove_flow(&mut self, path: Utf8PathBuf) {
        let flow_id = Self::flow_id(&path);
        self.flows.remove(&flow_id);
        info!(target: "flows", "Removed deleted flow {path}");
    }
}

#[derive(Default)]
struct FlowSpecs {
    flow_specs: HashMap<String, (Utf8PathBuf, FlowConfig)>,
}

impl FlowSpecs {
    pub async fn load(&mut self, config_dir: &Utf8Path) {
        let Ok(mut entries) = read_dir(config_dir).await.map_err(
            |err| error!(target: "flows", "Failed to read flows from {config_dir}: {err}"),
        ) else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "flows", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    if let Some("toml") = path.extension() {
                        info!(target: "flows", "Loading flow: {path}");
                        if let Err(err) = self.load_flow(path).await {
                            error!(target: "flows", "Failed to load flow: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn load_single_flow(&mut self, flow: &Path) {
        let Some(path) = Utf8Path::from_path(flow).map(|p| p.to_path_buf()) else {
            error!(target: "flows", "Skipping non UTF8 path: {}", flow.display());
            return;
        };
        if let Err(err) = self.load_flow(&path).await {
            error!(target: "flows", "Failed to load flow {path}: {err}");
        }
    }

    pub async fn load_single_script(&mut self, script: impl AsRef<Path>) {
        let script = script.as_ref();
        let Some(path) = Utf8Path::from_path(script).map(|p| p.to_path_buf()) else {
            error!(target: "flows", "Skipping non UTF8 path: {}", script.display());
            return;
        };
        let flow_id = MessageProcessor::flow_id(&path);
        let flow = FlowConfig::from_step(path.to_owned());
        self.flow_specs.insert(flow_id, (path.to_owned(), flow));
    }

    async fn load_flow(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let flow_id = MessageProcessor::flow_id(path);
        let specs = read_to_string(path).await?;
        let flow: FlowConfig = toml::from_str(&specs)?;
        self.flow_specs.insert(flow_id, (path.to_owned(), flow));

        Ok(())
    }

    async fn compile(
        mut self,
        js_runtime: &mut JsRuntime,
        config_dir: &Utf8Path,
    ) -> HashMap<String, Flow> {
        let mut flows = HashMap::new();
        for (name, (source, specs)) in self.flow_specs.drain() {
            match specs.compile(js_runtime, config_dir, source).await {
                Ok(flow) => {
                    let _ = flows.insert(name, flow);
                }
                Err(err) => {
                    error!(target: "flows", "Failed to compile flow {name}: {err}")
                }
            }
        }
        flows
    }
}

pub struct MeaDb<Timestamp, Payload> {
    keyspace: Keyspace,
    oldest: BTreeMap<String, Timestamp>,
    _payload: PhantomData<Payload>,
}

pub trait ToFromSlice {
    fn to_slice(&self) -> Slice;
    fn from_slice(slice: Slice) -> Self;
}

impl ToFromSlice for DateTime {
    fn to_slice(&self) -> Slice {
        let mut arr = [0u8; 12];
        let secs_bytes = self.seconds.to_be_bytes();
        let nanos_bytes = self.nanoseconds.to_be_bytes();

        arr[..8].copy_from_slice(&secs_bytes);
        arr[8..12].copy_from_slice(&nanos_bytes);

        Slice::new(&arr)
    }

    fn from_slice(slice: Slice) -> Self {
        let secs_be = &slice[..8];
        let nanos_be = &slice[8..];
        let secs = u64::from_be_bytes(secs_be.try_into().unwrap());
        let nanos = u32::from_be_bytes(nanos_be.try_into().unwrap());

        Self {
            seconds: secs,
            nanoseconds: nanos,
        }
    }
}

impl ToFromSlice for Message {
    fn to_slice(&self) -> Slice {
        Slice::new(self.json().to_string().as_bytes())
    }

    fn from_slice(slice: Slice) -> Self {
        serde_json::from_slice(&slice).unwrap()
    }
}

impl<Timestamp, Payload> MeaDb<Timestamp, Payload>
where
    Payload: ToFromSlice + Send + 'static,
    Timestamp: ToFromSlice + Ord + Copy + Send + 'static,
{
    pub async fn drain_older_than(
        &mut self,
        timestamp: Timestamp,
        series: &str,
    ) -> Result<Vec<(Timestamp, Payload)>, fjall::Error> {
        let ks = self.keyspace.clone();
        let (messages, new_oldest) = spawn_blocking({
            let series = series.to_owned();
            move || {
                let partition = ks.open_partition(&series, PartitionCreateOptions::default())?;
                let messages = partition
                    .range(..=timestamp.to_slice())
                    .map(|res| res.map(Self::decode))
                    .collect::<Result<Vec<_>, _>>()?;
                for msg in &messages {
                    partition.remove(msg.0.to_slice())?;
                }
                Ok::<_, fjall::Error>((messages, partition.first_key_value()?))
            }
        })
        .await
        .unwrap()?;

        self.oldest.remove(series);
        if let Some((ts, _payload)) = new_oldest {
            self.update_oldest(series, Timestamp::from_slice(ts));
        }
        Ok(messages)
    }

    fn decode((key, value): (Slice, Slice)) -> (Timestamp, Payload) {
        (Timestamp::from_slice(key), Payload::from_slice(value))
    }

    pub async fn open(path: impl AsRef<Path> + Send) -> Result<Self, fjall::Error> {
        let path = path.as_ref().to_owned();
        let keyspace = spawn_blocking(move || fjall::Config::new(path).open())
            .await
            .unwrap()?;
        Ok(Self {
            keyspace,
            oldest: <_>::default(),
            _payload: PhantomData,
        })
    }

    pub async fn store(
        &mut self,
        series: &str,
        timestamp: Timestamp,
        payload: Payload,
    ) -> Result<(), fjall::Error> {
        let result = spawn_blocking({
            let ks = self.keyspace.clone();
            let series = series.to_owned();
            move || {
                let partition = ks.open_partition(&series, PartitionCreateOptions::default())?;
                partition.insert(timestamp.to_slice(), payload.to_slice())?;
                Ok(())
            }
        })
        .await
        .unwrap();
        self.update_oldest(series, timestamp);
        result
    }

    fn update_oldest(&mut self, topic: &str, inserted_ts: Timestamp) {
        if let Some(value) = self.oldest.get_mut(topic) {
            *value = std::cmp::min(*value, inserted_ts)
        } else {
            self.oldest.insert(topic.to_owned(), inserted_ts);
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use std::path::PathBuf;

    // Helper function to create a dummy path
    fn dummy_path() -> PathBuf {
        PathBuf::from("/tmp/test_db")
    }

    impl ToFromSlice for String {
        fn to_slice(&self) -> Slice {
            Slice::new(self.as_bytes())
        }

        fn from_slice(slice: Slice) -> Self {
            String::from_utf8(slice.to_vec()).unwrap()
        }
    }

    #[tokio::test]
    async fn test_store_single_message() {
        let path = dummy_path();
        let mut db: MeaDb<DateTime, String> = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data";
        let timestamp = datetime!(2023-01-01 10:00 UTC).try_into().unwrap();
        let message = "temp: 25C".to_string();

        let result = db.store(series, timestamp, message.clone()).await;
        assert!(result.is_ok());

        // Verify the message was stored
        let stored_messages = db.drain_older_than(timestamp, series).await.unwrap();
        assert_eq!(stored_messages.len(), 1);
        assert_eq!(stored_messages[0], (timestamp, message));
    }

    #[tokio::test]
    async fn test_store_multiple_messages_same_series() {
        let path = dummy_path();
        let mut db: MeaDb<DateTime, String> = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data".to_string();
        let ts1 = datetime!(2023-01-01 10:00 UTC).try_into().unwrap();
        let msg1 = "temp: 25C".to_string();
        let ts2 = datetime!(2023-01-01 10:05 UTC).try_into().unwrap();
        let msg2 = "temp: 26C".to_string();
        let ts3 = datetime!(2023-01-01 09:55 UTC).try_into().unwrap();
        let msg3 = "temp: 24C".to_string();

        db.store(&series, ts1, msg1.clone()).await.unwrap();
        db.store(&series, ts2, msg2.clone()).await.unwrap();
        db.store(&series, ts3, msg3.clone()).await.unwrap();

        let stored_messages = db.drain_older_than(ts2, &series).await.unwrap();

        assert_eq!(stored_messages.len(), 3);
        // Verify messages are sorted by timestamp
        assert_eq!(stored_messages[0], (ts3, msg3));
        assert_eq!(stored_messages[1], (ts1, msg1));
        assert_eq!(stored_messages[2], (ts2, msg2));
    }

    #[tokio::test]
    async fn test_store_messages_different_series() {
        let path = dummy_path();
        let mut db: MeaDb<DateTime, String> = MeaDb::open(&path).await.unwrap();

        let series1 = "sensor_data_a".to_string();
        let ts1 = datetime!(2023-01-01 10:00 UTC).try_into().unwrap();
        let msg1 = "data A1".to_string();

        let series2 = "sensor_data_b".to_string();
        let ts2 = datetime!(2023-01-01 10:01 UTC).try_into().unwrap();
        let msg2 = "data B1".to_string();

        db.store(&series1, ts1, msg1.clone()).await.unwrap();
        db.store(&series2, ts2, msg2.clone()).await.unwrap();

        let s1_data = db.drain_older_than(ts1, &series1).await.unwrap();
        let s2_data = db.drain_older_than(ts2, &series2).await.unwrap();
        assert_eq!(s1_data.len(), 1);
        assert_eq!(s2_data.len(), 1);
    }

    #[tokio::test]
    async fn test_drain_removes_data() {
        let path = dummy_path();
        let mut db: MeaDb<DateTime, String> = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data_a".to_string();
        let timestamp = datetime!(2023-01-01 10:00 UTC).try_into().unwrap();
        let msg = "data A1".to_string();

        db.store(&series, timestamp, msg.clone()).await.unwrap();

        let data = db.drain_older_than(timestamp, &series).await.unwrap();
        assert_eq!(data.len(), 1);
        let data_after_drain = db.drain_older_than(timestamp, &series).await.unwrap();
        assert_eq!(data_after_drain.len(), 0);
    }
}
