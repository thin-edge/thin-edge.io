use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::js_value::JsonValue;
use crate::FlowError;
use crate::LoadError;
use crate::Message;
use camino::Utf8Path;
use std::time::Duration;
use std::time::SystemTime;
use tokio::time::Instant;

/// A message transformation step
pub struct FlowStep {
    handler: StepHandler,
    pub(crate) config: JsonValue,
    interval: Duration,
    pub(crate) next_execution: Option<Instant>,
}

pub enum StepHandler {
    JsScript(JsScript),
}

impl FlowStep {
    pub fn new_script(script: JsScript) -> Self {
        FlowStep {
            handler: StepHandler::JsScript(script),
            config: JsonValue::default(),
            interval: Duration::ZERO,
            next_execution: None,
        }
    }

    pub fn with_config(self, config: Option<serde_json::Value>) -> Self {
        if let Some(config) = config {
            Self {
                config: JsonValue::from(config),
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_interval(self, interval: Duration) -> Self {
        Self { interval, ..self }
    }

    /// Return source of this step (a path or a builtin transformer)
    pub fn source(&self) -> &str {
        match &self.handler {
            StepHandler::JsScript(script) => script.path.as_str(),
        }
    }

    /// Return the path to the source file of this step (if any, i.e. if not builtin)
    pub fn path(&self) -> Option<&Utf8Path> {
        match &self.handler {
            StepHandler::JsScript(script) => Some(&script.path),
        }
    }

    pub fn step_name(&self) -> &str {
        match &self.handler {
            StepHandler::JsScript(script) => &script.module_name,
        }
    }

    pub async fn load_script(&mut self, js: &mut JsRuntime) -> Result<(), LoadError> {
        match &mut self.handler {
            StepHandler::JsScript(script) => {
                js.load_script(script).await?;
                script.check(&self.interval);
                if !script.no_js_on_interval_fun && self.interval.is_zero() {
                    // Zero as a default is not appropriate for a script with an onInterval handler
                    self.interval = Duration::from_secs(1);
                }
            }
        }
        self.init_next_execution();
        Ok(())
    }

    /// Initialize the next execution time for this script's interval
    /// Should be called after the script is loaded and interval is set
    pub fn init_next_execution(&mut self) {
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
        &self,
        js: &JsRuntime,
        timestamp: SystemTime,
        message: &Message,
    ) -> Result<Vec<Message>, FlowError> {
        match &self.handler {
            StepHandler::JsScript(script) => {
                script
                    .on_message(js, timestamp, message, &self.config)
                    .await
            }
        }
    }

    /// Trigger the onInterval function of the JS module
    ///
    /// Return zero, one or more messages
    ///
    /// Note: Caller should check should_execute_interval() before calling this
    pub async fn on_interval(
        &self,
        js: &JsRuntime,
        timestamp: SystemTime,
    ) -> Result<Vec<Message>, FlowError> {
        match &self.handler {
            StepHandler::JsScript(script) => script.on_interval(js, timestamp, &self.config).await,
        }
    }
}
