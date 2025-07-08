use crate::config::PipelineConfig;
use crate::js_runtime::JsRuntime;
use crate::pipeline::DateTime;
use crate::pipeline::FilterError;
use crate::pipeline::Message;
use crate::pipeline::Pipeline;
use crate::pipeline::PipelineInput;
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
use std::path::PathBuf;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tokio::task::spawn_blocking;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct MessageProcessor {
    pub config_dir: PathBuf,
    pub pipelines: HashMap<String, Pipeline>,
    pub(super) js_runtime: JsRuntime,
    pub database: MeaDB,
}

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    Fjall(#[from] fjall::Error),
}

pub type MeaDB = MeaDb<DateTime, Message>;

impl MessageProcessor {
    pub fn pipeline_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs.load(&config_dir).await;
        let pipelines = pipeline_specs.compile(&mut js_runtime, &config_dir).await;

        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    fn db_path() -> Utf8PathBuf {
        "/etc/tedge/tedge-gen.db".into()
    }

    pub async fn try_new_single_pipeline(
        config_dir: impl AsRef<Path>,
        pipeline: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let pipeline = pipeline.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs.load_single_pipeline(&pipeline).await;
        let pipelines = pipeline_specs.compile(&mut js_runtime, &config_dir).await;
        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    pub async fn try_new_single_filter(
        config_dir: impl AsRef<Path>,
        filter: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref().to_owned();
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut pipeline_specs = PipelineSpecs::default();
        pipeline_specs.load_single_filter(&filter).await;
        let pipelines = pipeline_specs.compile(&mut js_runtime, &config_dir).await;
        Ok(MessageProcessor {
            config_dir,
            pipelines,
            js_runtime,
            database: MeaDb::open(Self::db_path()).await?,
        })
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for pipeline in self.pipelines.values() {
            topics.add_all(pipeline.topics())
        }
        topics
    }

    pub async fn process(
        &mut self,
        timestamp: &DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let mut out_messages = vec![];
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            let pipeline_output = pipeline.process(&self.js_runtime, timestamp, message).await;
            out_messages.push((pipeline_id.clone(), pipeline_output));
        }
        out_messages
    }

    pub async fn tick(
        &mut self,
        timestamp: &DateTime,
    ) -> Vec<(String, Result<Vec<Message>, FilterError>)> {
        let mut out_messages = vec![];
        for (pipeline_id, pipeline) in self.pipelines.iter_mut() {
            let pipeline_output = pipeline.tick(&self.js_runtime, timestamp).await;
            out_messages.push((pipeline_id.clone(), pipeline_output));
        }
        out_messages
    }

    pub async fn drain_db(
        &mut self,
        timestamp: &DateTime,
    ) -> Vec<(String, Result<Vec<(DateTime, Message)>, DatabaseError>)> {
        let mut out_messages = vec![];
        for (pipeline_id, pipeline) in self.pipelines.iter() {
            if let PipelineInput::MeaDB {
                series: input_series,
                frequency: input_frequency,
                max_age: input_span,
            } = &pipeline.input
            {
                if timestamp.tick_now(*input_frequency) {
                    let drained_messages = self
                        .database
                        .drain_older_than(timestamp.sub(input_span), input_series)
                        .await
                        .map_err(DatabaseError::from);
                    out_messages.push((pipeline_id.to_owned(), drained_messages));
                }
            }
        }
        out_messages
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn reload_filter(&mut self, path: Utf8PathBuf) {
        for pipeline in self.pipelines.values_mut() {
            for stage in &mut pipeline.stages {
                if stage.filter.path() == path {
                    match self
                        .js_runtime
                        .load_file(stage.filter.module_name(), &path)
                        .await
                    {
                        Ok(()) => {
                            info!(target: "gen-mapper", "Reloaded filter {path}");
                        }
                        Err(e) => {
                            error!(target: "gen-mapper", "Failed to reload filter {path}: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }

    pub async fn remove_filter(&mut self, path: Utf8PathBuf) {
        for (pipeline_id, pipeline) in self.pipelines.iter() {
            for stage in pipeline.stages.iter() {
                if stage.filter.path() == path {
                    warn!(target: "gen-mapper", "Removing a filter used by {pipeline_id}: {path}");
                    return;
                }
            }
        }
    }

    pub async fn load_pipeline(&mut self, pipeline_id: String, path: Utf8PathBuf) -> bool {
        let Ok(source) = tokio::fs::read_to_string(&path).await else {
            self.remove_pipeline(path).await;
            return false;
        };
        let config: PipelineConfig = match toml::from_str(&source) {
            Ok(config) => config,
            Err(e) => {
                error!(target: "gen-mapper", "Failed to parse toml for pipeline {path}: {e}");
                return false;
            }
        };
        match config
            .compile(&mut self.js_runtime, &self.config_dir, path.clone())
            .await
        {
            Ok(pipeline) => {
                self.pipelines.insert(pipeline_id, pipeline);
                true
            }
            Err(e) => {
                error!(target: "gen-mapper", "Failed to compile pipeline {path}: {e}");
                false
            }
        }
    }

    pub async fn add_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        if !self.pipelines.contains_key(&pipeline_id)
            && self.load_pipeline(pipeline_id, path.clone()).await
        {
            info!(target: "gen-mapper", "Loaded new pipeline {path}");
        }
    }

    pub async fn reload_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        if self.pipelines.contains_key(&pipeline_id)
            && self.load_pipeline(pipeline_id, path.clone()).await
        {
            info!(target: "gen-mapper", "Reloaded updated pipeline {path}");
        }
    }

    pub async fn remove_pipeline(&mut self, path: Utf8PathBuf) {
        let pipeline_id = Self::pipeline_id(&path);
        self.pipelines.remove(&pipeline_id);
        info!(target: "gen-mapper", "Removed deleted pipeline {path}");
    }
}

#[derive(Default)]
struct PipelineSpecs {
    pipeline_specs: HashMap<String, (Utf8PathBuf, PipelineConfig)>,
}

impl PipelineSpecs {
    pub async fn load(&mut self, config_dir: &PathBuf) {
        let Ok(mut entries) = read_dir(config_dir).await.map_err(|err|
            error!(target: "MAPPING", "Failed to read filters from {}: {err}", config_dir.display())
        ) else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let Some(path) = Utf8Path::from_path(&entry.path()).map(|p| p.to_path_buf()) else {
                error!(target: "MAPPING", "Skipping non UTF8 path: {}", entry.path().display());
                continue;
            };
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    if let Some("toml") = path.extension() {
                        info!(target: "MAPPING", "Loading pipeline: {path}");
                        if let Err(err) = self.load_pipeline(path).await {
                            error!(target: "MAPPING", "Failed to load pipeline: {err}");
                        }
                    }
                }
            }
        }
    }

    pub async fn load_single_pipeline(&mut self, pipeline: &Path) {
        let Some(path) = Utf8Path::from_path(pipeline).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", pipeline.display());
            return;
        };
        if let Err(err) = self.load_pipeline(&path).await {
            error!(target: "MAPPING", "Failed to load pipeline {path}: {err}");
        }
    }

    pub async fn load_single_filter(&mut self, filter: impl AsRef<Path>) {
        let filter = filter.as_ref();
        let Some(path) = Utf8Path::from_path(filter).map(|p| p.to_path_buf()) else {
            error!(target: "MAPPING", "Skipping non UTF8 path: {}", filter.display());
            return;
        };
        let pipeline_id = MessageProcessor::pipeline_id(&path);
        let pipeline = PipelineConfig::from_filter(path.to_owned());
        self.pipeline_specs
            .insert(pipeline_id, (path.to_owned(), pipeline));
    }

    async fn load_pipeline(&mut self, file: impl AsRef<Utf8Path>) -> Result<(), LoadError> {
        let path = file.as_ref();
        let pipeline_id = MessageProcessor::pipeline_id(path);
        let specs = read_to_string(path).await?;
        let pipeline: PipelineConfig = toml::from_str(&specs)?;
        self.pipeline_specs
            .insert(pipeline_id, (path.to_owned(), pipeline));

        Ok(())
    }

    async fn compile(
        mut self,
        js_runtime: &mut JsRuntime,
        config_dir: &Path,
    ) -> HashMap<String, Pipeline> {
        let mut pipelines = HashMap::new();
        for (name, (source, specs)) in self.pipeline_specs.drain() {
            match specs.compile(js_runtime, config_dir, source).await {
                Ok(pipeline) => {
                    let _ = pipelines.insert(name, pipeline);
                }
                Err(err) => {
                    error!(target: "MAPPING", "Failed to compile pipeline {name}: {err}")
                }
            }
        }
        pipelines
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
        *&mut arr[0..8].copy_from_slice(&self.seconds.to_be_bytes());
        *&mut arr[8..12].copy_from_slice(&self.nanoseconds.to_be_bytes());
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
        serde_json::from_slice(&*slice).unwrap()
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
            self.update_oldest(&series, Timestamp::from_slice(ts));
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
        self.update_oldest(&series, timestamp);
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
        let mut db = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data";
        let seconds = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let timestamp = DateTime {
            seconds: seconds as u64,
            nanoseconds: 0,
        };
        let message = "temp: 25C".to_string();

        let result = db.store(series, timestamp, message.clone()).await;
        assert!(result.is_ok());

        // Verify the message was stored
        let stored_messages = db.drain_older_than(timestamp, &series).await.unwrap();
        assert_eq!(stored_messages.len(), 1);
        assert_eq!(stored_messages[0], (timestamp, message));
    }

    #[tokio::test]
    async fn test_store_multiple_messages_same_series() {
        let path = dummy_path();
        let mut db = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data".to_string();
        let ts1 = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let ts1 = DateTime {
            seconds: ts1 as u64,
            nanoseconds: 0,
        };
        let msg1 = "temp: 25C".to_string();
        let ts2 = datetime!(2023-01-01 10:05 UTC).unix_timestamp();
        let ts2 = DateTime {
            seconds: ts2 as u64,
            nanoseconds: 0,
        };
        let msg2 = "temp: 26C".to_string();
        let ts3 = datetime!(2023-01-01 09:55 UTC).unix_timestamp();
        let ts3 = DateTime {
            seconds: ts3 as u64,
            nanoseconds: 0,
        };
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
        let mut db = MeaDb::open(&path).await.unwrap();

        let series1 = "sensor_data_a".to_string();
        let ts1 = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let ts1 = DateTime {
            seconds: ts1 as u64,
            nanoseconds: 0,
        };
        let msg1 = "data A1".to_string();

        let series2 = "sensor_data_b".to_string();
        let ts2 = datetime!(2023-01-01 10:01 UTC).unix_timestamp();
        let ts2 = DateTime {
            seconds: ts2 as u64,
            nanoseconds: 0,
        };
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
        let mut db = MeaDb::open(&path).await.unwrap();

        let series = "sensor_data_a".to_string();
        let timestamp = datetime!(2023-01-01 10:00 UTC).unix_timestamp();
        let timestamp = DateTime {
            seconds: timestamp as u64,
            nanoseconds: 0,
        };
        let msg = "data A1".to_string();

        db.store(&series, timestamp, msg.clone()).await.unwrap();

        let data = db.drain_older_than(timestamp, &series).await.unwrap();
        assert_eq!(data.len(), 1);
        let data_after_drain = db.drain_older_than(timestamp, &series).await.unwrap();
        assert_eq!(data_after_drain.len(), 0);
    }
}
