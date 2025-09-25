use crate::config::FlowConfig;
use crate::database;
use crate::database::DatabaseError;
use crate::database::MeaDb;
use cfg_if::cfg_if;

use crate::flow::DateTime;
use crate::flow::Flow;
use crate::flow::FlowError;
use crate::flow::Message;
use crate::flow::MessageSource;
use crate::js_runtime::JsRuntime;
use crate::stats::Counter;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_dir;
use tokio::fs::read_to_string;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct MessageProcessor {
    pub config_dir: Utf8PathBuf,
    pub flows: HashMap<String, Flow>,
    pub(super) js_runtime: JsRuntime,
    pub stats: Counter,
    pub database: Arc<Mutex<Box<dyn MeaDb>>>,
}

impl MessageProcessor {
    pub fn flow_id(path: impl AsRef<Path>) -> String {
        format!("{}", path.as_ref().display())
    }

    pub async fn try_new(config_dir: impl AsRef<Utf8Path>) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();

        let database = Self::create_database(config_dir).await?;

        Self::new_with_database(config_dir, database).await
    }

    pub async fn new_with_database(
        config_dir: impl AsRef<Utf8Path>,
        database: Box<dyn MeaDb>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let database = Arc::new(Mutex::new(database));
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load(config_dir).await;
        let flows = flow_specs
            .compile(&mut js_runtime, config_dir, database.clone())
            .await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database,
        })
    }

    fn db_path(config_dir: &Utf8Path) -> Utf8PathBuf {
        config_dir.join("tedge-flows.db")
    }

    async fn create_database(config_dir: &Utf8Path) -> Result<Box<dyn MeaDb>, DatabaseError> {
        cfg_if! {
            if #[cfg(feature = "fjall-db")] {
                Ok(Box::new(database::FjallMeaDb::open(&Self::db_path(config_dir)).await?))
            } else if #[cfg(feature = "sqlite-db")] {
                Ok(Box::new(database::SqliteMeaDb::open(&Self::db_path(config_dir)).await?))
            } else {
                compile_error!("Either 'fjall-db' or 'sqlite-db' feature must be enabled");
            }
        }
    }

    pub async fn try_new_single_flow(
        config_dir: impl AsRef<Utf8Path>,
        flow: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let flow = flow.as_ref().to_owned();

        let database = Self::create_database(config_dir).await?;

        Self::new_single_flow_with_database(config_dir, flow, database).await
    }

    async fn new_single_flow_with_database(
        config_dir: impl AsRef<Utf8Path>,
        flow: impl AsRef<Path>,
        database: Box<dyn MeaDb>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let flow = flow.as_ref().to_owned();
        let database = Arc::new(Mutex::new(database));
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_flow(&flow).await;
        let flows = flow_specs
            .compile(&mut js_runtime, config_dir, database.clone())
            .await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database,
        })
    }

    pub async fn try_new_single_step_flow(
        config_dir: impl AsRef<Utf8Path>,
        script: impl AsRef<Path>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();

        let database = Self::create_database(config_dir).await?;

        Self::new_single_step_flow_with_database(config_dir, script, database).await
    }

    async fn new_single_step_flow_with_database(
        config_dir: impl AsRef<Utf8Path>,
        script: impl AsRef<Path>,
        database: Box<dyn MeaDb>,
    ) -> Result<Self, LoadError> {
        let config_dir = config_dir.as_ref();
        let database = Arc::new(Mutex::new(database));
        let mut js_runtime = JsRuntime::try_new().await?;
        let mut flow_specs = FlowSpecs::default();
        flow_specs.load_single_script(&script).await;
        let flows = flow_specs
            .compile(&mut js_runtime, config_dir, database.clone())
            .await;
        let stats = Counter::default();

        Ok(MessageProcessor {
            config_dir: config_dir.to_owned(),
            flows,
            js_runtime,
            stats,
            database,
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
        let script_deadlines = self
            .flows
            .values()
            .flat_map(|flow| &flow.steps)
            .filter_map(|step| step.script.next_execution);

        let source_deadlines = self
            .flows
            .values()
            .filter_map(|flow| flow.input_source.as_ref()?.next_deadline());

        script_deadlines.chain(source_deadlines)
    }

    /// Get the next deadline for interval execution across all scripts and input sources
    /// Returns None if no scripts have intervals configured and no input sources are scheduled
    pub fn next_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().min()
    }

    /// Get the last deadline for interval execution across all scripts
    /// Returns None if no scripts have intervals configured
    ///
    /// This is intended for `tedge flows test` to ensure it processes all
    /// intervals
    pub fn last_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines().max()
    }

    pub async fn on_message(
        &mut self,
        source: MessageSource,
        timestamp: DateTime,
        message: &Message,
    ) -> Vec<(String, Result<Vec<Message>, FlowError>)> {
        let started_at = self.stats.runtime_on_message_start();

        let mut out_messages = vec![];
        for (flow_id, flow) in self.flows.iter_mut() {
            let flow_output = flow
                .on_message(
                    &self.js_runtime,
                    source,
                    &mut self.stats,
                    timestamp,
                    message,
                )
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

    /// Poll input sources that are ready at the given timestamp and return drained messages
    /// This is primarily for testing purposes
    pub async fn poll_input_sources(
        &mut self,
        timestamp: DateTime,
    ) -> Vec<(
        String,
        Result<Vec<(DateTime, Message)>, crate::input_source::InputSourceError>,
    )> {
        let mut results = vec![];

        for (flow_id, flow) in &mut self.flows {
            if let Some(source) = &mut flow.input_source {
                if source.is_ready(timestamp) {
                    let messages = source.poll(timestamp).await;
                    source.update_after_poll(timestamp);
                    results.push((flow_id.clone(), messages));
                }
            }
        }

        results
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
            .compile(
                &mut self.js_runtime,
                &self.config_dir,
                path.clone(),
                self.database.clone(),
            )
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
        database: Arc<Mutex<Box<dyn MeaDb>>>,
    ) -> HashMap<String, Flow> {
        let mut flows = HashMap::new();
        for (name, (source, specs)) in self.flow_specs.drain() {
            match specs
                .compile(js_runtime, config_dir, source, database.clone())
                .await
            {
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use time::macros::datetime;

    use super::*;
    use crate::database::InMemoryMeaDb;
    use camino::Utf8PathBuf;

    #[tokio::test]
    async fn message_processor_stores_message_to_database() {
        let (processor, _temp_dir) = create_test_processor_with_memory_db().await;

        let series = "test_series";
        let timestamp = DateTime::try_from(datetime!(2023-01-01 10:00 UTC)).unwrap();
        let message = crate::flow::Message {
            topic: "test/topic".to_string(),
            payload: r#"{"value": 42}"#.into(),
            timestamp: Some(timestamp),
        };

        processor
            .database
            .lock()
            .await
            .store(series, timestamp, message.clone())
            .await
            .expect("store should succeed");

        let stored_messages = processor
            .database
            .lock()
            .await
            .query_all(series)
            .await
            .unwrap();
        assert_eq!(stored_messages, [(timestamp, message)]);
    }

    #[tokio::test]
    async fn message_processor_drains_messages_from_database() {
        let (processor, _temp_dir) = create_test_processor_with_memory_db().await;

        let series = "test_series";
        let timestamp = DateTime::try_from(datetime!(2023-01-01 10:00 UTC)).unwrap();
        let message = crate::flow::Message {
            topic: "test/topic".to_string(),
            payload: r#"{"value": 42}"#.into(),
            timestamp: Some(timestamp),
        };

        // Store a message first
        processor
            .database
            .lock()
            .await
            .store(series, timestamp, message.clone())
            .await
            .unwrap();

        // Drain the message
        let drained_messages = processor
            .database
            .lock()
            .await
            .drain_older_than(timestamp, series)
            .await
            .unwrap();
        assert_eq!(drained_messages, [(timestamp, message)]);

        // Verify database is empty after drain
        let remaining_messages = processor
            .database
            .lock()
            .await
            .query_all(series)
            .await
            .unwrap();
        assert_eq!(remaining_messages, []);
    }

    async fn create_test_processor_with_memory_db() -> (MessageProcessor, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap();

        let database = Box::new(InMemoryMeaDb::default());
        let processor = MessageProcessor::new_with_database(config_dir, database)
            .await
            .expect("Failed to create MessageProcessor");

        (processor, temp_dir)
    }
}
