use crate::config::ConfigError;
use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::LoadError;
use crate::Message;
use camino::Utf8Path;
use std::fmt::Display;
use std::time::Duration;
use std::time::SystemTime;
use tokio::time::Instant;

/// A message transformation step
pub struct FlowStep {
    handler: StepHandler,
    interval: Duration,
    pub(crate) next_execution: Option<Instant>,
}

pub enum StepHandler {
    JsScript(JsScript, JsonValue),
    Transformer(String, Box<dyn Transformer>),
}

impl FlowStep {
    /// Return a name that uniquely identifies a step instance
    ///
    /// This name is built after : the flow name, the script name (or builtin transformer)
    /// and the index that step among all the steps of the flow (so two instances of the same script
    /// in the same flow are given different instance name).
    pub fn instance_name(flow: impl Display, script: impl Display, index: usize) -> String {
        format!("{flow}|{index}|{script}")
    }

    pub fn new_script(script: JsScript) -> Self {
        let config = JsonValue::default();
        FlowStep {
            handler: StepHandler::JsScript(script, config),
            interval: Duration::ZERO,
            next_execution: None,
        }
    }

    pub fn new_transformer(instance_name: String, transformer: Box<dyn Transformer>) -> Self {
        FlowStep {
            handler: StepHandler::Transformer(instance_name, transformer),
            interval: Duration::ZERO,
            next_execution: None,
        }
    }

    pub fn with_config(mut self, config: Option<serde_json::Value>) -> Result<Self, ConfigError> {
        if let Some(config) = config {
            self.handler.set_config(JsonValue::from(config))?
        };
        Ok(self)
    }

    pub fn with_interval(mut self, interval: Option<Duration>, flow: &str) -> Self {
        let is_periodic = match &self.handler {
            StepHandler::JsScript(script, _) => script.is_periodic,
            StepHandler::Transformer(_, builtin) => builtin.is_periodic(),
        };
        if !is_periodic && interval.is_some() {
            tracing::warn!(target: "flows", "Script with no 'onInterval' function: {}; but configured with an 'interval' in {flow}", self.source());
        }
        let interval = interval.unwrap_or_else(|| {
            if is_periodic {
                Duration::from_secs(1)
            } else {
                Duration::ZERO
            }
        });

        self.interval = interval;
        self.init_next_execution();
        self
    }

    /// Return source of this step (a path or a builtin transformer)
    pub fn source(&self) -> &str {
        match &self.handler {
            StepHandler::JsScript(script, _) => script.path.as_str(),
            StepHandler::Transformer(_, builtin) => builtin.name(),
        }
    }

    /// Return the path to the source file of this step (if any, i.e. if not builtin)
    pub fn path(&self) -> Option<&Utf8Path> {
        match &self.handler {
            StepHandler::JsScript(script, _) => Some(&script.path),
            StepHandler::Transformer(_, _) => None,
        }
    }

    pub fn step_name(&self) -> &str {
        match &self.handler {
            StepHandler::JsScript(script, _) => &script.module_name,
            StepHandler::Transformer(instance_name, _) => instance_name,
        }
    }

    pub async fn load_script(&mut self, js: &mut JsRuntime) -> Result<(), LoadError> {
        if let StepHandler::JsScript(script, _) = &mut self.handler {
            js.load_script(script).await?;
            // FIXME: there is bug here when the updated version adds an on_interval method
            // This method will be ignored, because the interval is zero (because there no on_interval method before)
            // => The configured interval must not be erased
            self.init_next_execution();
        }
        Ok(())
    }

    /// Initialize the next execution time for this script's interval
    /// Should be called after the script is loaded and interval is set
    fn init_next_execution(&mut self) {
        if !self.interval.is_zero() {
            self.next_execution = Some(Instant::now() + self.interval);
        }
    }

    /// Check if this script should execute its interval function now
    /// Returns true and updates next_execution if it's time to execute
    pub fn should_execute_interval(&mut self, now: Instant) -> bool {
        if self.interval.is_zero() {
            return false;
        }

        match self.next_execution {
            Some(deadline) if now >= deadline => {
                // Time to execute - schedule next execution
                self.next_execution = Some(now + self.interval);
                true
            }
            None => {
                // First execution - initialize and execute
                self.next_execution = Some(now + self.interval);
                true
            }
            _ => false,
        }
    }

    /// Transform an input message into zero, one or more output messages
    pub async fn on_message(
        &mut self,
        js: &JsRuntime,
        timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        match &mut self.handler {
            StepHandler::JsScript(script, config) => {
                script.on_message(js, timestamp, message, config).await
            }
            StepHandler::Transformer(_, builtin) => {
                builtin.on_message(timestamp, message, &js.context_handle())
            }
        }
    }

    /// Trigger the onInterval function of the JS module
    ///
    /// Return zero, one or more messages
    ///
    /// Note: Caller should check should_execute_interval() before calling this
    pub async fn on_interval(
        &mut self,
        js: &JsRuntime,
        timestamp: SystemTime,
    ) -> Result<Vec<Message>, FlowError> {
        match &mut self.handler {
            StepHandler::JsScript(script, config) => {
                script.on_interval(js, timestamp, config).await
            }
            StepHandler::Transformer(_, builtin) => {
                builtin.on_interval(timestamp, &js.context_handle())
            }
        }
    }
}

impl StepHandler {
    pub fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        match self {
            StepHandler::JsScript(_, ref mut c) => *c = config,
            StepHandler::Transformer(_, ref mut builtin) => {
                builtin.set_config(config)?;
            }
        }

        Ok(())
    }
}
