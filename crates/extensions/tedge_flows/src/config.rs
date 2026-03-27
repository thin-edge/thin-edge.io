use crate::flow::Flow;
use crate::flow::FlowInput;
use crate::flow::FlowOutput;
use crate::js_runtime::JsRuntime;
use crate::js_script::JsScript;
use crate::params::Params;
use crate::steps::FlowStep;
use crate::transformers::BuiltinTransformers;
use crate::LoadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use glob::glob;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::Duration;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tokio::fs::read_to_string;
use tracing::error;
use tracing::info;

#[derive(Deserialize)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub struct FlowConfig {
    // meta info
    version: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,

    /// configuration shared by the steps of this flow
    #[serde(default)]
    config: Map<String, Value>,

    input: InputConfig,
    #[serde(default)]
    steps: Vec<StepConfig>,
    #[serde(default = "default_output")]
    output: OutputConfig,
    #[serde(default = "default_errors")]
    errors: OutputConfig,

    /// If true, output messages that match the input filter are not dropped
    #[serde(default)]
    expect_loop: bool,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub struct StepConfig {
    #[serde(flatten)]
    step: StepSpec,

    #[serde(default)]
    config: Map<String, Value>,

    #[serde(default)]
    #[serde(deserialize_with = "parse_human_interval")]
    interval: Option<IntervalConfig>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub enum StepSpec {
    #[serde(rename = "builtin")]
    Transformer(String),

    #[serde(rename = "script")]
    JavaScript(Utf8PathBuf),
}

#[derive(Clone, Deserialize)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub enum InputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topics: Vec<String> },

    #[serde(rename = "file")]
    File {
        path: Utf8PathBuf,

        /// Default to path
        topic: Option<String>,

        #[serde(default)]
        #[serde(deserialize_with = "parse_human_interval")]
        interval: Option<IntervalConfig>,
    },

    #[serde(rename = "process")]
    Process {
        command: String,

        /// Default to command
        topic: Option<String>,

        #[serde(default)]
        #[serde(deserialize_with = "parse_human_interval")]
        interval: Option<IntervalConfig>,
    },
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub enum OutputConfig {
    #[serde(rename = "mqtt")]
    Mqtt { topic: Option<String> },

    #[serde(rename = "file")]
    File { path: Utf8PathBuf },
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
pub enum IntervalConfig {
    Duration(Duration),
    ParamExpr(String),
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Not a valid filename for a flow")]
    IncorrectFlowFilename,

    #[error("Not a valid MQTT topic: {0}")]
    IncorrectTopic(String),

    #[error("Not a valid MQTT topic filter: {0}")]
    IncorrectTopicFilter(String),

    #[error(transparent)]
    LoadError(#[from] LoadError),

    #[error("Not a valid step configuration: {0}")]
    IncorrectSetting(String),

    #[error("Not a valid interval duration: {0}")]
    IncorrectInterval(String),

    #[error("Flow '{name}' defines an infinite loop: the output topic '{output_topic}' matches input filter '{input_filter}'")]
    MqttInfiniteLoop {
        name: String,
        input_filter: String,
        output_topic: String,
    },

    #[error("Flow '{name}' defines an infinite loop: the output file '{path}' is the same as the input file")]
    FileInfiniteLoop { name: String, path: String },
}

/// ```
/// use tedge_flows::derive_flow_name;
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/flow.toml".into()), Some("flow".into()));
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello.toml".into()), Some("hello".into()));
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello/flow.toml".into()), Some("hello".into()));
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello/world.toml".into()), Some("hello/world".into()));
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello/world/flow.toml".into()), Some("hello/world".into()));
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/.toml".into()), None);
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello/params.toml".into()), None);
/// assert_eq!(derive_flow_name("/flows".into(), "/flows/hello/world.js".into()), None);
/// assert_eq!(derive_flow_name("/flows".into(), "/unrelated/flows/hello.toml".into()), None);
/// ```
pub fn derive_flow_name(flows_dir: &Utf8Path, flow_path: &Utf8Path) -> Option<String> {
    let path = flow_path.strip_prefix(flows_dir).ok()?;
    if path.extension() != Some("toml") {
        return None;
    };
    if path.file_name()? == Params::filename() {
        return None;
    }
    match (path.parent()?.as_str(), path.file_stem()?) {
        ("", "flow") => Some("flow".to_string()),
        (dir_name, "flow") => Some(dir_name.into()),
        ("", file_stem) => Some(file_stem.into()),
        (dir_name, file_stem) => Some(format!("{dir_name}/{file_stem}")),
    }
}

impl FlowConfig {
    /// Loads all the flow definitions
    ///
    /// Return the collection of loaded flow configs
    /// as well as the list of files that cannot be read as flow specs
    pub async fn load_all_flows(
        flows_dir: &Utf8Path,
    ) -> (HashMap<Utf8PathBuf, FlowConfig>, Vec<Utf8PathBuf>) {
        let pattern = format!("{}/**/*.toml", flows_dir);
        let paths = tokio::task::spawn_blocking(move || {
            let mut paths = Vec::new();
            match glob(&pattern) {
                Ok(entries) => {
                    for entry in entries.filter_map(Result::ok) {
                        let Some(path) = Utf8Path::from_path(entry.as_path()).map(|p| p.to_path_buf()) else {
                            error!(target: "flows", "Skipping non UTF8 path: {}", entry.as_path().display());
                            continue;
                        };
                        if path.is_file() && !Params::is_params_file(&path) {
                            paths.push(path);
                        }
                    }
                }
                Err(err) => {
                    error!(target: "flows", "Failed to glob pattern {}: {}", pattern, err);
                }
            }
            paths
        })
        .await
        .unwrap_or_default();

        let mut flows = HashMap::new();
        let mut unloaded_flows = Vec::new();
        for path in paths {
            info!(target: "flows", "Loading flow: {path}");
            if let Some(flow) = FlowConfig::load_single_flow(&path).await {
                flows.insert(path, flow);
            } else {
                unloaded_flows.push(path.clone());
            }
        }
        (flows, unloaded_flows)
    }

    pub async fn load_single_flow(flow: &Utf8Path) -> Option<FlowConfig> {
        match FlowConfig::load_flow(flow).await {
            Ok(flow) => Some(flow),
            Err(err) => {
                error!(target: "flows", "Failed to load flow {flow}: {err}");
                None
            }
        }
    }

    pub fn wrap_script_into_flow(script: &Utf8Path) -> FlowConfig {
        FlowConfig::from_step(script.to_owned())
    }

    async fn load_flow(path: &Utf8Path) -> Result<FlowConfig, ConfigError> {
        let specs = read_to_string(path)
            .await
            .map_err(|err| LoadError::from_io(err, path))?;
        let flow: FlowConfig = toml::from_str(&specs).map_err(LoadError::from)?;

        let params = Params::load_flow_params(path).await?;
        flow.substitute_params(&params)
    }

    fn substitute_params(mut self, params: &Params) -> Result<Self, ConfigError> {
        self.config = params.substitute_all(&self.config)?;
        for step in self.steps.iter_mut() {
            step.substitute_params(params)?;
        }

        Ok(FlowConfig {
            input: self.input.substitute_params(params)?,
            output: self.output.substitute_params(params)?,
            errors: self.errors.substitute_params(params)?,
            ..self
        })
    }

    pub fn from_step(script: Utf8PathBuf) -> Self {
        let input_topic = "#".to_string();
        let step = StepConfig {
            step: StepSpec::JavaScript(script),
            config: Map::new(),
            interval: None,
        };
        Self {
            version: None,
            description: None,
            tags: None,
            config: Map::new(),
            // Expect a loop when wrapping a single script as a flow, as there is no way to statically identify input and output topics
            expect_loop: true,
            input: InputConfig::Mqtt {
                topics: vec![input_topic],
            },
            steps: vec![step],
            output: default_output(),
            errors: default_errors(),
        }
    }

    pub async fn compile(
        self,
        rs_transformers: &BuiltinTransformers,
        js_runtime: &mut JsRuntime,
        flows_dir: &Utf8Path,
        source: Utf8PathBuf,
    ) -> Result<Flow, ConfigError> {
        let input = self.input.try_into()?;
        let output = self.output.try_into()?;
        let errors = self.errors.try_into()?;
        let mut steps = vec![];
        for (i, step) in self.steps.into_iter().enumerate() {
            let step = step
                .with_shared_config(&self.config)
                .with_interval_as_config()
                .compile(rs_transformers, js_runtime, i, &source)
                .await?;
            steps.push(step);
        }

        let Some(name) = derive_flow_name(flows_dir, &source) else {
            return Err(ConfigError::IncorrectFlowFilename);
        };

        detect_loop(&name, &input, &output, self.expect_loop)?;

        Ok(Flow {
            name,
            version: self.version,
            description: self.description,
            tags: self.tags,
            input,
            steps,
            output,
            errors,
            source,
            expect_loop: self.expect_loop,
        })
    }
}

impl StepConfig {
    pub fn substitute_params(&mut self, params: &Params) -> Result<(), ConfigError> {
        self.config = params.substitute_all(&self.config)?;
        if let Some(interval) = self.interval.take() {
            self.interval = Some(interval.substitute_params(params)?);
        }
        Ok(())
    }

    pub fn interval(&self) -> Result<Option<Duration>, ConfigError> {
        self.interval.as_ref().map(|i| i.duration()).transpose()
    }

    pub fn with_shared_config(mut self, shared_config: &Map<String, Value>) -> Self {
        for (k, v) in shared_config.iter() {
            if !self.config.contains_key(k) {
                self.config.insert(k.clone(), v.clone());
            }
        }
        self
    }

    pub fn with_interval_as_config(mut self) -> Self {
        let key = "interval";
        let interval = self
            .interval()
            .ok()
            .flatten()
            .unwrap_or(Duration::from_secs(1));
        if !self.config.contains_key(key) {
            self.config
                .insert(key.to_string(), interval.as_secs().into());
        }
        self
    }

    pub async fn compile(
        &self,
        rs_transformers: &BuiltinTransformers,
        js_runtime: &mut JsRuntime,
        index: usize,
        flow: &Utf8Path,
    ) -> Result<FlowStep, ConfigError> {
        let step = match &self.step {
            StepSpec::JavaScript(path) => {
                Self::compile_script(js_runtime, flow, path, index).await?
            }
            StepSpec::Transformer(name) => {
                Self::instantiate_builtin(rs_transformers, flow, name, index)?
            }
        };
        let config = if self.config.is_empty() {
            None
        } else {
            Some(Value::Object(self.config.clone()))
        };
        let step = step
            .with_config(config)?
            .with_interval(self.interval()?, flow.as_str());
        Ok(step)
    }

    async fn compile_script(
        js_runtime: &mut JsRuntime,
        flow: &Utf8Path,
        path: &Utf8Path,
        index: usize,
    ) -> Result<FlowStep, ConfigError> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            // path relative to the flow definition, fallback to existing path
            flow.parent()
                .map(|parent| parent.join(path))
                .unwrap_or_else(|| path.to_owned())
        };
        let path = path
            .canonicalize_utf8()
            .unwrap_or_else(|_| path.to_path_buf());
        let module_name = FlowStep::instance_name(flow, &path, index);
        let mut script = JsScript::new(module_name, flow.to_owned(), path);
        js_runtime.load_script(&mut script).await?;
        Ok(FlowStep::new_script(script))
    }

    fn instantiate_builtin(
        rs_transformers: &BuiltinTransformers,
        flow: &Utf8Path,
        name: &String,
        index: usize,
    ) -> Result<FlowStep, ConfigError> {
        let instance_name = FlowStep::instance_name(flow, name, index);
        let transformer = rs_transformers.new_instance(name)?;
        Ok(FlowStep::new_transformer(instance_name, transformer))
    }
}

impl InputConfig {
    fn substitute_params(self, params: &Params) -> Result<Self, ConfigError> {
        match self {
            InputConfig::Mqtt { topics } => Ok(InputConfig::Mqtt {
                topics: topics
                    .into_iter()
                    .map(|t| params.substitute_inner_paths(&t))
                    .collect(),
            }),
            InputConfig::File {
                path,
                topic,
                interval,
            } => Ok(InputConfig::File {
                path,
                topic: topic.map(|t| params.substitute_inner_paths(&t)),
                interval: interval.map(|i| i.substitute_params(params)).transpose()?,
            }),
            InputConfig::Process {
                command,
                topic,
                interval,
            } => Ok(InputConfig::Process {
                command: params.substitute_inner_paths(&command),
                topic: topic.map(|t| params.substitute_inner_paths(&t)),
                interval: interval.map(|i| i.substitute_params(params)).transpose()?,
            }),
        }
    }
}

impl TryFrom<InputConfig> for FlowInput {
    type Error = ConfigError;
    fn try_from(input: InputConfig) -> Result<Self, Self::Error> {
        Ok(match input {
            InputConfig::Mqtt { topics } => FlowInput::Mqtt {
                topics: topic_filters(topics)?,
            },
            InputConfig::File {
                topic,
                path,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| path.clone().to_string());
                match interval.map(|i| i.duration()) {
                    Some(Ok(interval)) if !interval.is_zero() => FlowInput::PollFile {
                        topic,
                        path,
                        interval,
                    },
                    Some(Err(e)) => return Err(e),
                    _ => FlowInput::StreamFile { topic, path },
                }
            }
            InputConfig::Process {
                topic,
                command,
                interval,
            } => {
                let topic = topic.unwrap_or_else(|| command.clone());
                match interval.map(|i| i.duration()) {
                    Some(Ok(interval)) if !interval.is_zero() => FlowInput::PollCommand {
                        topic,
                        command,
                        interval,
                    },
                    Some(Err(e)) => return Err(e),
                    _ => FlowInput::StreamCommand { topic, command },
                }
            }
        })
    }
}

impl OutputConfig {
    fn substitute_params(self, params: &Params) -> Result<Self, LoadError> {
        match self {
            OutputConfig::Mqtt { topic } => Ok(OutputConfig::Mqtt {
                topic: topic.map(|t| params.substitute_inner_paths(&t)),
            }),
            OutputConfig::File { path } => Ok(OutputConfig::File { path }),
        }
    }
}

impl TryFrom<OutputConfig> for FlowOutput {
    type Error = ConfigError;

    fn try_from(input: OutputConfig) -> Result<Self, Self::Error> {
        Ok(match input {
            OutputConfig::Mqtt { topic } => FlowOutput::Mqtt {
                topic: topic.map(into_topic).transpose()?,
            },
            OutputConfig::File { path } => FlowOutput::File { path },
        })
    }
}

impl IntervalConfig {
    fn substitute_params(self, params: &Params) -> Result<Self, ConfigError> {
        match &self {
            IntervalConfig::Duration(_) => Ok(self),
            IntervalConfig::ParamExpr(expr) => {
                let interval = params.substitute_inner_paths(expr);
                let duration = humantime::parse_duration(&interval)
                    .map_err(|_| ConfigError::IncorrectInterval(interval))?;
                Ok(IntervalConfig::Duration(duration))
            }
        }
    }

    fn duration(&self) -> Result<Duration, ConfigError> {
        match self {
            IntervalConfig::Duration(duration) => Ok(*duration),
            IntervalConfig::ParamExpr(expr) => Err(ConfigError::IncorrectInterval(expr.clone())),
        }
    }
}

fn into_topic(name: String) -> Result<Topic, ConfigError> {
    Topic::new(&name).map_err(|_| ConfigError::IncorrectTopic(name))
}

pub(crate) fn topic_filters<S: AsRef<str> + ToString>(
    patterns: Vec<S>,
) -> Result<TopicFilter, ConfigError> {
    let mut topics = TopicFilter::empty();
    for pattern in patterns {
        topics
            .try_add(pattern.as_ref())
            .map_err(|_| ConfigError::IncorrectTopicFilter(pattern.to_string()))?;
    }
    Ok(topics)
}

fn parse_human_interval<'de, D>(deserializer: D) -> Result<Option<IntervalConfig>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.trim().is_empty() {
        Ok(None)
    } else if let Ok(duration) = humantime::parse_duration(&value) {
        Ok(Some(IntervalConfig::Duration(duration)))
    } else {
        Ok(Some(IntervalConfig::ParamExpr(value)))
    }
}

fn default_output() -> OutputConfig {
    OutputConfig::Mqtt { topic: None }
}

fn default_errors() -> OutputConfig {
    OutputConfig::Mqtt {
        topic: Some("te/error".to_string()),
    }
}

/// Checks whether `input` and `output` form an infinite loop,
/// where the output of published to the same input source.
/// When `expect_loop` is true, the check is skipped.
fn detect_loop(
    name: &str,
    input: &FlowInput,
    output: &FlowOutput,
    expect_loop: bool,
) -> Result<(), ConfigError> {
    if expect_loop {
        return Ok(());
    }
    match (input, output) {
        (FlowInput::Mqtt { topics }, FlowOutput::Mqtt { topic: Some(out) })
            if topics.accept_topic_name(&out.name) =>
        {
            Err(ConfigError::MqttInfiniteLoop {
                name: name.to_string(),
                input_filter: format!("{topics:?}"),
                output_topic: out.name.clone(),
            })
        }
        (
            FlowInput::PollFile { path: in_path, .. } | FlowInput::StreamFile { path: in_path, .. },
            FlowOutput::File { path: out_path },
        ) if in_path == out_path => Err(ConfigError::FileInfiniteLoop {
            name: name.to_string(),
            path: in_path.to_string(),
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use serde_json::json;
    use std::time::Duration;
    use tedge_mqtt_ext::Topic;
    use tedge_mqtt_ext::TopicFilter;
    use test_case::test_case;

    #[test]
    fn inherit_shared_config() {
        for (shared_config, step_config, merged_config) in [
            (json!({}), json!({"x": 1, "y": 2}), json!({"x": 1, "y": 2})),
            (
                json!({"z": 3}),
                json!({"x": 1, "y": 2}),
                json!({"x": 1, "y": 2, "z": 3}),
            ),
            (
                json!({"z": 3, "x": 4}),
                json!({"x": 1, "y": 2}),
                json!({"x": 1, "y": 2, "z": 3}),
            ),
            (
                json!({"x": 4}),
                json!({"x": 1, "y": 2}),
                json!({"x": 1, "y": 2}),
            ),
            (json!({"x": 4}), json!({}), json!({"x": 4})),
        ] {
            let shared_config = shared_config.as_object().unwrap();
            let step_config = step_config.as_object().unwrap();
            let merged_config = merged_config.as_object().unwrap();
            let step = StepConfig {
                step: StepSpec::Transformer("some-step".to_string()),
                config: step_config.clone(),
                interval: None,
            };
            assert_eq!(
                &step.with_shared_config(shared_config).config,
                merged_config
            );
        }
    }

    #[test]
    fn with_interval_as_config() {
        for (interval, config, merged_config) in [
            (None, json!({}), json!({"interval": 1})),
            (
                Some(Duration::from_secs(5)),
                json!({}),
                json!({"interval": 5}),
            ),
            (None, json!({"x": 42}), json!({"interval": 1, "x": 42})),
            (
                Some(Duration::from_secs(5)),
                json!({"x": 42}),
                json!({"interval": 5, "x": 42}),
            ),
            (
                Some(Duration::from_secs(5)),
                json!({"interval": 33, "x": 42}),
                json!({"interval": 33, "x": 42}),
            ),
        ] {
            let config = config.as_object().unwrap();
            let merged_config = merged_config.as_object().unwrap();
            let step = StepConfig {
                step: StepSpec::Transformer("some-step".to_string()),
                config: config.clone(),
                interval: interval.map(IntervalConfig::Duration),
            };
            assert_eq!(&step.with_interval_as_config().config, merged_config);
        }
    }

    #[test]
    fn detect_loop_when_output_topic_matches_input_filter() {
        let input = FlowInput::Mqtt {
            topics: TopicFilter::new_unchecked("te/loop/+"),
        };
        let output = FlowOutput::Mqtt {
            topic: Some(Topic::new("te/loop/test").unwrap()),
        };
        assert!(matches!(
            detect_loop("my-flow", &input, &output, false),
            Err(ConfigError::MqttInfiniteLoop { .. })
        ));
    }

    #[test]
    fn detect_loop_when_poll_file_output_matches_input_path() {
        let input = FlowInput::PollFile {
            topic: "te/loop".to_string(),
            path: Utf8PathBuf::from("/tmp/data.txt"),
            interval: Duration::from_secs(1),
        };
        let output = FlowOutput::File {
            path: Utf8PathBuf::from("/tmp/data.txt"),
        };
        assert!(matches!(
            detect_loop("my-flow", &input, &output, false),
            Err(ConfigError::FileInfiniteLoop { .. })
        ));
    }

    #[test]
    fn detect_loop_when_stream_file_output_matches_input_path() {
        let input = FlowInput::StreamFile {
            topic: "te/loop".to_string(),
            path: Utf8PathBuf::from("/tmp/data.txt"),
        };
        let output = FlowOutput::File {
            path: Utf8PathBuf::from("/tmp/data.txt"),
        };
        assert!(matches!(
            detect_loop("my-flow", &input, &output, false),
            Err(ConfigError::FileInfiniteLoop { .. })
        ));
    }

    #[test]
    fn expect_loop_suppresses_loop_detection() {
        let input = FlowInput::Mqtt {
            topics: TopicFilter::new_unchecked("te/loop/+"),
        };
        let output = FlowOutput::Mqtt {
            topic: Some(Topic::new("te/loop/test").unwrap()),
        };
        // Would normally be an error, but expect_loop = true bypasses the check
        assert!(detect_loop("my-flow", &input, &output, true).is_ok());
    }

    #[test_case(FlowOutput::Mqtt { topic: Some(Topic::new("te/another/test").unwrap()) }; "mqtt output different topic"
    )]
    #[test_case(FlowOutput::Mqtt { topic: None }; "mqtt output without fixed topic")]
    fn no_loop_detected_for_different_output_types(output: FlowOutput) {
        let input = FlowInput::Mqtt {
            topics: TopicFilter::new_unchecked("te/loop/+"),
        };
        assert!(detect_loop("my-flow", &input, &output, false).is_ok());
    }

    #[test]
    fn params_substitution() {
        let params_toml = r#"
        topic.in = "continuous-deployments/deployments/default/rollout"
        topic.out = "te/device/main///e/"
        group_name = "foo-group-name"
        arch = "x86_64"
        "#;

        let flow_toml = r#"
        [input.process]
topic = "${params.topic.in}"
command = """tedge http post /c8y/service/foo/${params.group_name} --data '{"arch":"${params.arch}"}'"""
interval = "3600s"

[[steps]]
script = "main.js"

[output.mqtt]
topic = "${params.topic.out}"
"#;

        let expected_flow_toml = r#"
        [input.process]
topic = "continuous-deployments/deployments/default/rollout"
command = """tedge http post /c8y/service/foo/foo-group-name --data '{"arch":"x86_64"}'"""
interval = "3600s"

[[steps]]
script = "main.js"

[output.mqtt]
topic = "te/device/main///e/"
"#;

        let params = Params::load_toml(params_toml).unwrap();
        let flow: FlowConfig = toml::from_str(flow_toml).unwrap();
        let expected_flow: FlowConfig = toml::from_str(expected_flow_toml).unwrap();

        assert_eq!(expected_flow, flow.substitute_params(&params).unwrap());
    }

    #[test]
    fn params_substitute_mqtt_topics() {
        let params_toml = r#"
        topic.in = "input"
        topic.out = "output"
        child = "child-xyz"
        "#;

        let flow_toml = r#"
        input.mqtt.topics = [ "${params.topic.in}", "c8y/#", "te/device/${params.child}///e/"]
        output.mqtt.topic = "c8y/${params.topic.out}"
        "#;

        let expected_flow_toml = r#"
        input.mqtt.topics = [ "input", "c8y/#", "te/device/child-xyz///e/"]
        output.mqtt.topic = "c8y/output"
        "#;

        let params = Params::load_toml(params_toml).unwrap();
        let flow: FlowConfig = toml::from_str(flow_toml).unwrap();
        let expected_flow: FlowConfig = toml::from_str(expected_flow_toml).unwrap();

        assert_eq!(expected_flow, flow.substitute_params(&params).unwrap());
    }

    #[test]
    fn params_substitute_intervals() {
        let params_toml = r#"
        hourly = "3600s"
        daily = "24h"
        "#;

        let flow_toml = r#"
        [input.process]
        command = "/some/command"
        interval = "${params.daily}"

        [[steps]]
        script = "main.js"
        interval = "${params.hourly}"
        "#;

        let expected_flow_toml = r#"
        [input.process]
        command = "/some/command"
        interval = "24h"

        [[steps]]
        script = "main.js"
        interval = "3600s"
        "#;

        let params = Params::load_toml(params_toml).unwrap();
        let flow: FlowConfig = toml::from_str(flow_toml).unwrap();
        let expected_flow: FlowConfig = toml::from_str(expected_flow_toml).unwrap();

        assert_eq!(expected_flow, flow.substitute_params(&params).unwrap());
    }
}
