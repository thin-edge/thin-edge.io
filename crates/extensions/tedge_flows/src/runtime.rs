use crate::flow::FlowResult;
use crate::flow::Message;
use crate::flow::SourceTag;
use crate::js_runtime::JsRuntime;
use crate::registry::BaseFlowRegistry;
use crate::registry::FlowRegistryExt;
use crate::stats::Counter;
use crate::FlowError;
use crate::FlowOutput;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::time::SystemTime;
use tedge_mqtt_ext::TopicFilter;
use tokio::time::Instant;

pub struct MessageProcessor<Registry> {
    pub registry: Registry,
    pub js_runtime: JsRuntime,
    pub stats: Counter,
}

impl MessageProcessor<BaseFlowRegistry> {
    pub async fn with_base_registry(config_dir: impl AsRef<Utf8Path>) -> Result<Self, LoadError> {
        let registry = BaseFlowRegistry::new(config_dir);
        Self::try_new(registry).await
    }
}

impl<Registry: FlowRegistryExt + Send> MessageProcessor<Registry> {
    pub async fn try_new(registry: Registry) -> Result<Self, LoadError> {
        let js_runtime = JsRuntime::try_new().await?;
        let stats = Counter::default();

        Ok(MessageProcessor {
            registry,
            js_runtime,
            stats,
        })
    }

    pub async fn load_all_flows(&mut self) {
        self.registry.load_all_flows(&mut self.js_runtime).await;
    }

    pub async fn load_single_flow(&mut self, flow: impl AsRef<Utf8Path>) {
        self.registry
            .load_single_flow(&mut self.js_runtime, flow.as_ref())
            .await;
    }

    pub async fn load_single_script(&mut self, script: impl AsRef<Utf8Path>) {
        self.registry
            .load_single_script(&mut self.js_runtime, script.as_ref())
            .await;
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = TopicFilter::empty();
        for flow in self.registry.flows() {
            topics.add_all(flow.as_ref().topics())
        }
        topics
    }

    /// Get the next deadline for interval execution across all scripts
    /// Returns None if no scripts have intervals configured
    pub fn next_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.registry.deadlines().min()
    }

    /// Get the last deadline for interval execution across all scripts Returns
    /// None if no scripts have intervals configured
    ///
    /// This is intended for `tedge flows test` to ensure it processes all
    /// intervals
    pub fn last_interval_deadline(&self) -> Option<tokio::time::Instant> {
        self.registry.deadlines().max()
    }

    pub async fn on_flow_input(
        &mut self,
        flow_name: &str,
        timestamp: SystemTime,
        message: &Message,
    ) -> Option<FlowResult> {
        let flow = self.registry.flow_mut(flow_name)?;
        let started_at = self.stats.runtime_on_message_start();
        let flow_output = flow
            .as_mut()
            .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
            .await;
        self.stats.runtime_on_message_done(started_at);
        Some(flow_output)
    }

    pub async fn on_message(
        &mut self,
        timestamp: SystemTime,
        source: &SourceTag,
        message: &Message,
    ) -> Vec<FlowResult> {
        let started_at = self.stats.runtime_on_message_start();

        let mut out_messages = vec![];
        for flow in self.registry.flows_mut() {
            let config_result = flow
                .as_mut()
                .on_config_update(&self.js_runtime, message)
                .await;
            if config_result.is_err() {
                out_messages.push(config_result);
                continue;
            }
            if flow.as_ref().accept_message(source, message) {
                let flow_output = flow
                    .as_mut()
                    .on_message(&self.js_runtime, &mut self.stats, timestamp, message)
                    .await;
                out_messages.push(flow_output);
            }
        }

        self.stats.runtime_on_message_done(started_at);
        out_messages
            .into_iter()
            .filter_map(|flow_output| self.store_context_values(flow_output))
            .collect()
    }

    pub async fn on_interval(&mut self, timestamp: SystemTime, now: Instant) -> Vec<FlowResult> {
        let mut out_messages = vec![];
        for flow in self.registry.flows_mut() {
            let flow_output = flow
                .as_mut()
                .on_interval(&self.js_runtime, &mut self.stats, timestamp, now)
                .await;
            out_messages.push(flow_output);
        }
        out_messages
    }

    fn store_context_values(&mut self, messages: FlowResult) -> Option<FlowResult> {
        match messages {
            FlowResult::Ok {
                messages,
                output: FlowOutput::Context,
                flow,
            } => {
                for message in messages {
                    if let Err(error) = self.store_context_value(&message) {
                        return Some(FlowResult::Err {
                            flow,
                            error,
                            output: FlowOutput::Context,
                        });
                    }
                }
                None
            }
            messages => Some(messages),
        }
    }

    pub fn store_context_value(&mut self, message: &Message) -> Result<(), FlowError> {
        if message.payload.is_empty() {
            self.js_runtime.store.remove(&message.topic)
        } else {
            let payload = message.payload_str().ok_or(FlowError::UnsupportedMessage(
                "Non UFT8 payload".to_string(),
            ))?;
            let value: serde_json::Value = serde_json::from_str(payload)
                .map_err(|err| FlowError::UnsupportedMessage(format!("Non JSON payload: {err}")))?;
            self.js_runtime
                .store
                .insert(message.topic.to_owned(), value);
        }

        Ok(())
    }

    pub async fn dump_processing_stats(&self) {
        self.stats.dump_processing_stats();
    }

    pub async fn dump_memory_stats(&self) {
        self.js_runtime.dump_memory_stats().await;
    }

    pub async fn reload_script(&mut self, path: Utf8PathBuf) {
        self.registry
            .reload_script(&mut self.js_runtime, path)
            .await;
    }

    pub async fn remove_script(&mut self, path: Utf8PathBuf) {
        self.registry.remove_script(path).await;
    }

    pub async fn add_flow(&mut self, path: Utf8PathBuf) {
        self.registry.add_flow(&mut self.js_runtime, &path).await;
    }

    pub async fn remove_flow(&mut self, path: Utf8PathBuf) {
        self.registry.remove_flow(&path).await;
    }
}
