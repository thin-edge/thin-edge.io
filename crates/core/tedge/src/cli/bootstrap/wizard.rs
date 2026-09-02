//! The interactive bootstrap wizard
//!
//! A gather-then-compile front-end:
//! all answers are collected up front from the cloud descriptors,
//! the equivalent non-interactive command is printed,
//! and then the exact same pipeline runs as for that command.
//! No prompting happens once the pipeline has started.
//!
//! A run that skips the wizard (the cloud and its URL are already known)
//! but still has to prompt for the registration method's missing inputs
//! goes through [`collect_missing_inputs`] and prints its equivalent
//! command the same way.

use super::cli::KeyValue;
use super::descriptor;
use super::descriptor::CloudDescriptor;
use super::descriptor::RegisterMethod;
use super::invocation::Invocation;
use anyhow::bail;
use anyhow::Context;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Write;

/// The wizard's answers, in the same shape as the CLI arguments
#[cfg_attr(test, derive(Debug))]
pub struct WizardAnswers {
    pub cloud: String,
    pub register: Option<String>,
    pub url: Option<String>,
    pub device_id: Option<String>,
    pub settings: Vec<KeyValue>,
    /// Registration inputs collected interactively,
    /// passed to the register hooks as environment variables
    pub hook_envs: Vec<(String, String)>,
}

/// Answers already provided (via CLI flags, or the positional cloud):
/// the wizard skips the corresponding questions
/// and carries the values through into its answers
#[derive(Default)]
pub struct WizardSeed {
    /// The cloud, by its descriptor name
    pub cloud: Option<String>,
    pub url: Option<String>,
    pub register: Option<String>,
    pub device_id: Option<String>,
    /// Config keys already provided via --set (full keys, e.g. `thingsboard.transport.port`)
    pub set_keys: Vec<String>,
}

/// A lookup of the environment the registration inputs may come from
type EnvLookup = Box<dyn Fn(&str) -> Option<String> + Send>;

/// The wizard's console: questions out, answers in.
///
/// Wraps the standard streams for real runs, and scripted answers in tests
pub struct Prompter {
    input: Box<dyn BufRead + Send>,
    output: Box<dyn Write + Send>,
    /// Read secrets without echo (only possible on a terminal)
    secrets_from_tty: bool,
    env: EnvLookup,
}

impl Prompter {
    /// Questions on stderr, answers from stdin
    pub fn stdio() -> Self {
        Self {
            input: Box::new(std::io::BufReader::new(std::io::stdin())),
            output: Box::new(std::io::stderr()),
            secrets_from_tty: std::io::stdin().is_terminal(),
            env: Box::new(descriptor::env_var),
        }
    }

    /// A prompter over arbitrary streams and environment (echoing secrets)
    #[cfg(test)]
    pub fn new(
        input: impl BufRead + Send + 'static,
        output: impl Write + Send + 'static,
        env: impl Fn(&str) -> Option<String> + Send + 'static,
    ) -> Self {
        Self {
            input: Box::new(input),
            output: Box::new(output),
            secrets_from_tty: false,
            env: Box::new(env),
        }
    }

    fn say(&mut self, text: &str) {
        let _ = writeln!(self.output, "{text}");
    }

    /// Numbered single-choice prompt; empty input selects the default
    fn choice(&mut self, options: &[(String, String)], default: usize) -> anyhow::Result<usize> {
        let width = options
            .iter()
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0);
        for (i, (name, description)) in options.iter().enumerate() {
            self.say(&format!("  {}) {name:width$}  {description}", i + 1));
        }
        loop {
            let answer = self.line(&format!("Choice [{}]", default + 1))?;
            if answer.is_empty() {
                return Ok(default);
            }
            match answer.parse::<usize>() {
                Ok(n) if (1..=options.len()).contains(&n) => return Ok(n - 1),
                _ => self.say(&format!("Enter a number between 1 and {}", options.len())),
            }
        }
    }

    /// Prompt for a value that may have a declared default or a choice list.
    ///
    /// A choice list is presented as a numbered pick-list
    /// (the default choice pre-selected);
    /// otherwise the default is shown in brackets
    /// and an empty answer accepts it.
    fn input(
        &mut self,
        prompt: &str,
        default: Option<&str>,
        choices: &[String],
        secret: bool,
    ) -> anyhow::Result<Option<String>> {
        if !choices.is_empty() {
            self.say(&format!("{prompt}:"));
            let labels: Vec<(String, String)> = choices
                .iter()
                .map(|choice| (choice.clone(), String::new()))
                .collect();
            let default_index = default
                .and_then(|default| choices.iter().position(|choice| choice == default))
                .unwrap_or(0);
            let index = self.choice(&labels, default_index)?;
            return Ok(Some(choices[index].clone()));
        }
        let display = match default {
            // never echo a secret's default value in the prompt
            // (a site override may have replaced it with a real secret)
            Some(_) if secret => format!("{prompt} [default set]"),
            Some(default) => format!("{prompt} [{default}]"),
            None => prompt.to_owned(),
        };
        let value = self.value(&display, secret)?;
        Ok(value.or_else(|| default.map(str::to_owned)))
    }

    /// Prompt for a value; empty input yields None.
    /// Secrets are read without echo when a terminal is available.
    fn value(&mut self, prompt: &str, secret: bool) -> anyhow::Result<Option<String>> {
        let value = if secret && self.secrets_from_tty {
            rpassword::prompt_password(format!("{prompt}: ")).context("Failed to read input")?
        } else {
            self.line(prompt)?
        };
        Ok(Some(value).filter(|value| !value.is_empty()))
    }

    fn line(&mut self, prompt: &str) -> anyhow::Result<String> {
        let _ = write!(self.output, "{prompt}: ");
        self.output.flush().ok();
        let mut line = String::new();
        let read = self
            .input
            .read_line(&mut line)
            .context("Failed to read input")?;
        if read == 0 {
            bail!("Input closed before the wizard was complete");
        }
        Ok(line.trim().to_owned())
    }
}

pub fn run(
    descriptors: &[CloudDescriptor],
    seed: &WizardSeed,
    prompter: &mut Prompter,
) -> anyhow::Result<WizardAnswers> {
    // A cloud without a descriptor (custom mapper) gets generic prompts
    let fallback_descriptor;
    let descriptor = match &seed.cloud {
        Some(name) => match descriptors.iter().find(|d| d.cloud == *name) {
            Some(descriptor) => descriptor,
            None => {
                fallback_descriptor = CloudDescriptor {
                    cloud: name.clone(),
                    cloud_type: None,
                    description: name.clone(),
                    register: Vec::new(),
                    url: None,
                    settings: Vec::new(),
                    set_config: Default::default(),
                    hidden: false,
                };
                &fallback_descriptor
            }
        },
        None => {
            // Hidden clouds (clouds.d ignore markers) are not offered;
            // naming one explicitly still works
            let offered: Vec<&CloudDescriptor> = descriptors
                .iter()
                .filter(|descriptor| !descriptor.hidden)
                .collect();
            if offered.is_empty() {
                bail!("All clouds are hidden by clouds.d ignore markers; name one explicitly");
            }
            prompter.say("Select the cloud to bootstrap:");
            let cloud_labels: Vec<(String, String)> = offered
                .iter()
                .map(|descriptor| (descriptor.cloud.clone(), descriptor.description.clone()))
                .collect();
            let cloud_index = prompter.choice(&cloud_labels, 0)?;
            offered[cloud_index]
        }
    };
    let cloud = descriptor.cloud.clone();

    let register = if let Some(register) = &seed.register {
        Some(register.clone())
    } else if descriptor.register.len() > 1 {
        prompter.say("\nHow should the device obtain its credentials?");
        let method_labels: Vec<(String, String)> = descriptor
            .register
            .iter()
            .map(|method| (method.name.clone(), method.description.clone()))
            .collect();
        let default = descriptor
            .register
            .iter()
            .position(|method| method.default)
            .unwrap_or(0);
        let index = prompter.choice(&method_labels, default)?;
        Some(descriptor.register[index].name.clone())
    } else {
        descriptor.register.first().map(|m| m.name.clone())
    };

    let url = match &descriptor.url {
        _ if seed.url.is_some() => seed.url.clone(),
        Some(spec) if spec.fixed_value().is_some() => {
            let url = spec.fixed_value().unwrap().to_owned();
            prompter.say(&format!(
                "URL: {url} (fixed by the {cloud} cloud descriptor)"
            ));
            Some(url)
        }
        Some(spec) => {
            let prompt = if spec.description.is_empty() {
                format!("{} URL", descriptor.description)
            } else {
                spec.description.clone()
            };
            prompter.input(&prompt, spec.default.as_deref(), &spec.choices, false)?
        }
        None => prompter.value(&format!("{} URL", descriptor.description), false)?,
    };

    let device_id = match &seed.device_id {
        Some(device_id) => Some(device_id.clone()),
        None => prompter.value(
            "Device ID (used as the device identity; leave empty to use an existing one)",
            false,
        )?,
    };

    let mut settings = Vec::new();
    for setting in &descriptor.settings {
        let full_key = if setting.global {
            setting.key.clone()
        } else {
            format!("{cloud}.{}", setting.key)
        };
        if seed.set_keys.contains(&full_key) {
            continue;
        }
        let prompt = setting.name.clone().unwrap_or_else(|| {
            if setting.description.is_empty() {
                setting.key.clone()
            } else {
                format!("{} ({})", setting.key, setting.description)
            }
        });
        if !setting.choices.is_empty() {
            // A pick-list question, presented in the descriptor's vocabulary
            // (labels and descriptions); the chosen value — and any further
            // config it implies — still compiles to plain --set pairs
            prompter.say(&format!("\n{prompt}:"));
            let labels: Vec<(String, String)> = setting
                .choices
                .iter()
                .map(|choice| (choice.label().to_owned(), choice.description().to_owned()))
                .collect();
            let default_index = setting
                .default
                .as_deref()
                .and_then(|default| {
                    setting
                        .choices
                        .iter()
                        .position(|choice| choice.value() == default)
                })
                .unwrap_or(0);
            let index = prompter.choice(&labels, default_index)?;
            let choice = &setting.choices[index];
            settings.push(KeyValue {
                key: full_key,
                value: choice.value().to_owned(),
            });
            for (key, value) in choice.implied_config().into_iter().flatten() {
                let implied_key = format!("{cloud}.{key}");
                // an explicit --set always wins over an implied value
                if seed.set_keys.contains(&implied_key) {
                    continue;
                }
                settings.push(KeyValue {
                    key: implied_key,
                    value: value.clone(),
                });
            }
            continue;
        }
        let value = prompter.input(&prompt, setting.default.as_deref(), &[], false)?;
        match value {
            Some(value) => settings.push(KeyValue {
                key: full_key,
                value,
            }),
            None if setting.required => bail!("A value for {} is required", setting.key),
            None => {}
        }
    }

    let mut hook_envs = Vec::new();
    if let Some(method) = register.as_deref().and_then(|name| descriptor.method(name)) {
        for input in &method.inputs {
            if (prompter.env)(&input.env).is_some() {
                prompter.say(&format!("  {} taken from ${}", input.name, input.env));
                continue;
            }
            if let Some(value) = prompt_input(prompter, method, input)? {
                hook_envs.push((input.env.clone(), value));
            }
        }
    }

    Ok(WizardAnswers {
        cloud,
        register,
        url,
        device_id,
        settings,
        hook_envs,
    })
}

/// Prompt for a method's missing required inputs —
/// the narrow interactive path for runs that will register
/// but did not go through the full wizard
/// (e.g. a --re-register run on an already-configured device)
pub fn collect_missing_inputs(
    method: &RegisterMethod,
    existing: &[(String, String)],
    prompter: &mut Prompter,
) -> anyhow::Result<Vec<(String, String)>> {
    let mut collected = Vec::new();
    for input in descriptor::missing_inputs(method, existing, &prompter.env) {
        let value = prompt_input(prompter, method, input)?
            .expect("a required input without a default is either answered or an error");
        collected.push((input.env.clone(), value));
    }
    Ok(collected)
}

/// Ask for one registration input; a required input without a default
/// must be answered
fn prompt_input(
    prompter: &mut Prompter,
    method: &RegisterMethod,
    input: &descriptor::MethodInput,
) -> anyhow::Result<Option<String>> {
    if !input.description.is_empty() {
        prompter.say(&input.description);
    }
    let prompt = format!("{} (${})", input.name, input.env);
    let value = prompter.input(
        &prompt,
        input.default.as_deref(),
        &input.choices,
        input.secret,
    )?;
    if value.is_none() && input.is_required() {
        bail!(
            "The {} registration method requires {} (${})",
            method.name,
            input.name,
            input.env
        );
    }
    Ok(value)
}

/// Print the equivalent non-interactive command of an interactive run,
/// so the wizard teaches the automatable form of what it is about to do.
///
/// The invocation is the run as resolved (instance name, profile, type,
/// and the answers compiled to flags), which is also what `--save` writes:
/// the printed command and a saved invocation never diverge.
pub fn print_equivalent_command(invocation: &Invocation) {
    eprintln!("\nRunning: {}", invocation.command_line());
    if !invocation.env.is_empty() {
        eprintln!(
            "(with the environment variables: {})",
            invocation.env.join(", ")
        );
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::bootstrap::descriptor::builtin_descriptors;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::Mutex;

    /// A writer whose content can be inspected after the prompter took it
    #[derive(Clone, Default)]
    struct Transcript(Arc<Mutex<Vec<u8>>>);

    impl Write for Transcript {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Transcript {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn scripted(answers: &str, env: &[(&str, &str)]) -> (Prompter, Transcript) {
        let transcript = Transcript::default();
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let prompter = Prompter::new(
            Cursor::new(answers.to_owned()),
            transcript.clone(),
            move |name| env.get(name).cloned(),
        );
        (prompter, transcript)
    }

    #[test]
    fn the_full_wizard_compiles_answers_to_flags() {
        // cloud 1 (c8y); default method (c8y-ca); url; device id;
        // MQTT connection type choice 2; one-time password left empty
        let (mut prompter, transcript) =
            scripted("1\n\nexample.cumulocity.com\ndemo01\n2\n\n", &[]);
        let answers = run(
            &builtin_descriptors(),
            &WizardSeed::default(),
            &mut prompter,
        )
        .unwrap();
        assert_eq!(answers.cloud, "c8y");
        assert_eq!(answers.register.as_deref(), Some("c8y-ca"));
        assert_eq!(answers.url.as_deref(), Some("example.cumulocity.com"));
        assert_eq!(answers.device_id.as_deref(), Some("demo01"));
        assert_eq!(
            answers.settings,
            vec![KeyValue {
                key: "c8y.mqtt_service.enabled".into(),
                value: "true".into()
            }]
        );
        // an optional input left empty is not an answer
        assert!(answers.hook_envs.is_empty());
        let text = transcript.text();
        assert!(text.contains("Select the cloud to bootstrap"), "{text}");
        assert!(text.contains("MQTT Service"), "{text}");
    }

    #[test]
    fn seeded_answers_skip_their_questions_and_inputs_come_from_the_environment() {
        // only the device id and the MQTT setting are asked;
        // the basic method's password is taken from the environment,
        // its user from the descriptor default (empty answer)
        let (mut prompter, transcript) =
            scripted("demo01\n\n\n", &[("C8Y_BOOTSTRAP_PASSWORD", "secret")]);
        let seed = WizardSeed {
            cloud: Some("c8y".into()),
            url: Some("example.cumulocity.com".into()),
            register: Some("basic".into()),
            device_id: None,
            set_keys: vec![],
        };
        let answers = run(&builtin_descriptors(), &seed, &mut prompter).unwrap();
        assert_eq!(answers.register.as_deref(), Some("basic"));
        assert_eq!(answers.url.as_deref(), Some("example.cumulocity.com"));
        assert_eq!(
            answers.hook_envs,
            vec![(
                "C8Y_BOOTSTRAP_USER".to_owned(),
                "management/devicebootstrap".to_owned()
            )]
        );
        let text = transcript.text();
        assert!(!text.contains("Select the cloud"), "{text}");
        assert!(
            text.contains("taken from $C8Y_BOOTSTRAP_PASSWORD"),
            "{text}"
        );
    }

    #[test]
    fn settings_given_with_set_are_not_asked_again() {
        let (mut prompter, transcript) = scripted("demo01\n\n", &[]);
        let seed = WizardSeed {
            cloud: Some("c8y".into()),
            url: Some("example.cumulocity.com".into()),
            register: Some("c8y-ca".into()),
            device_id: None,
            set_keys: vec!["c8y.mqtt_service.enabled".into()],
        };
        let answers = run(&builtin_descriptors(), &seed, &mut prompter).unwrap();
        assert!(answers.settings.is_empty());
        assert!(!transcript.text().contains("MQTT connection type"));
    }

    #[test]
    fn a_missing_required_input_is_an_error() {
        let (mut prompter, _) = scripted("demo01\n\n\n\n", &[]);
        let seed = WizardSeed {
            cloud: Some("c8y".into()),
            url: Some("example.cumulocity.com".into()),
            register: Some("basic".into()),
            device_id: None,
            set_keys: vec![],
        };
        let err = run(&builtin_descriptors(), &seed, &mut prompter).unwrap_err();
        assert!(err.to_string().contains("$C8Y_BOOTSTRAP_PASSWORD"), "{err}");
    }

    #[test]
    fn invalid_choices_are_asked_again_and_eof_is_an_error() {
        let (mut prompter, transcript) = scripted("9\nx\n2\n", &[]);
        let options = vec![
            ("a".to_owned(), String::new()),
            ("b".to_owned(), String::new()),
        ];
        assert_eq!(prompter.choice(&options, 0).unwrap(), 1);
        assert_eq!(transcript.text().matches("Enter a number").count(), 2);

        let err = prompter.choice(&options, 0).unwrap_err();
        assert!(err.to_string().contains("Input closed"), "{err}");
    }

    #[test]
    fn custom_clouds_without_a_descriptor_get_generic_prompts() {
        let (mut prompter, transcript) = scripted("tb.example.com\ndemo01\n", &[]);
        let seed = WizardSeed {
            cloud: Some("thingsboard".into()),
            ..Default::default()
        };
        let answers = run(&builtin_descriptors(), &seed, &mut prompter).unwrap();
        assert_eq!(answers.cloud, "thingsboard");
        assert_eq!(answers.register, None);
        assert_eq!(answers.url.as_deref(), Some("tb.example.com"));
        assert!(transcript.text().contains("thingsboard URL"));
    }

    #[test]
    fn only_the_missing_inputs_of_a_method_are_collected() {
        let method = builtin_descriptors()
            .into_iter()
            .find(|d| d.cloud == "c8y")
            .unwrap()
            .method("basic-preregistered")
            .unwrap()
            .clone();
        let (mut prompter, transcript) = scripted("pw\n", &[("C8Y_DEVICE_USER", "t1/device_x")]);
        let collected = collect_missing_inputs(&method, &[], &mut prompter).unwrap();
        assert_eq!(
            collected,
            vec![("C8Y_DEVICE_PASSWORD".to_owned(), "pw".to_owned())]
        );
        assert!(!transcript.text().contains("C8Y_DEVICE_USER"));
    }
}
