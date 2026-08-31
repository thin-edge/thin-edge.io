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
use super::descriptor::CloudDescriptor;
use super::invocation::Invocation;
use anyhow::bail;
use anyhow::Context;
use std::io::IsTerminal;
use std::io::Write;

/// The wizard's answers, in the same shape as the CLI arguments
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

pub fn run(descriptors: &[CloudDescriptor], seed: &WizardSeed) -> anyhow::Result<WizardAnswers> {
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
            eprintln!("Select the cloud to bootstrap:");
            let cloud_labels: Vec<(String, String)> = offered
                .iter()
                .map(|descriptor| (descriptor.cloud.clone(), descriptor.description.clone()))
                .collect();
            let cloud_index = prompt_choice(&cloud_labels, 0)?;
            offered[cloud_index]
        }
    };
    let cloud = descriptor.cloud.clone();

    let register = if let Some(register) = &seed.register {
        Some(register.clone())
    } else if descriptor.register.len() > 1 {
        eprintln!("\nHow should the device obtain its credentials?");
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
        let index = prompt_choice(&method_labels, default)?;
        Some(descriptor.register[index].name.clone())
    } else {
        descriptor.register.first().map(|m| m.name.clone())
    };

    let url = match &descriptor.url {
        _ if seed.url.is_some() => seed.url.clone(),
        Some(spec) if spec.fixed_value().is_some() => {
            let url = spec.fixed_value().unwrap().to_owned();
            eprintln!("URL: {url} (fixed by the {cloud} cloud descriptor)");
            Some(url)
        }
        Some(spec) => {
            let prompt = if spec.description.is_empty() {
                format!("{} URL", descriptor.description)
            } else {
                spec.description.clone()
            };
            prompt_input(&prompt, spec.default.as_deref(), &spec.choices, false)?
        }
        None => prompt_value(&format!("{} URL", descriptor.description), false)?,
    };

    let device_id = match &seed.device_id {
        Some(device_id) => Some(device_id.clone()),
        None => prompt_value(
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
            eprintln!("\n{prompt}:");
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
            let index = prompt_choice(&labels, default_index)?;
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
        let value = prompt_input(&prompt, setting.default.as_deref(), &[], false)?;
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
            if std::env::var(&input.env).is_ok_and(|value| !value.is_empty()) {
                eprintln!("  {} taken from ${}", input.name, input.env);
                continue;
            }
            if !input.description.is_empty() {
                eprintln!("{}", input.description);
            }
            let prompt = format!("{} (${})", input.name, input.env);
            let value = prompt_input(
                &prompt,
                input.default.as_deref(),
                &input.choices,
                input.secret,
            )?;
            match value {
                Some(value) => hook_envs.push((input.env.clone(), value)),
                None if input.is_required() => {
                    bail!(
                        "The {} registration method requires {} (${})",
                        method.name,
                        input.name,
                        input.env
                    )
                }
                None => {}
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
    method: &super::descriptor::RegisterMethod,
    existing: &[(String, String)],
) -> anyhow::Result<Vec<(String, String)>> {
    let mut collected = Vec::new();
    for input in super::descriptor::missing_inputs(method, existing) {
        if !input.description.is_empty() {
            eprintln!("{}", input.description);
        }
        let prompt = format!("{} (${})", input.name, input.env);
        let value = prompt_input(
            &prompt,
            input.default.as_deref(),
            &input.choices,
            input.secret,
        )?;
        match value {
            Some(value) => collected.push((input.env.clone(), value)),
            None => bail!(
                "The {} registration method requires {} (${})",
                method.name,
                input.name,
                input.env
            ),
        }
    }
    Ok(collected)
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

/// Numbered single-choice prompt; empty input selects the default
fn prompt_choice(options: &[(String, String)], default: usize) -> anyhow::Result<usize> {
    let width = options
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0);
    for (i, (name, description)) in options.iter().enumerate() {
        eprintln!("  {}) {name:width$}  {description}", i + 1);
    }
    loop {
        let answer = prompt_line(&format!("Choice [{}]", default + 1))?;
        if answer.is_empty() {
            return Ok(default);
        }
        match answer.parse::<usize>() {
            Ok(n) if (1..=options.len()).contains(&n) => return Ok(n - 1),
            _ => eprintln!("Enter a number between 1 and {}", options.len()),
        }
    }
}

/// Prompt for a value that may have a declared default or a choice list.
///
/// A choice list is presented as a numbered pick-list
/// (the default choice pre-selected);
/// otherwise the default is shown in brackets
/// and an empty answer accepts it.
fn prompt_input(
    prompt: &str,
    default: Option<&str>,
    choices: &[String],
    secret: bool,
) -> anyhow::Result<Option<String>> {
    if !choices.is_empty() {
        eprintln!("{prompt}:");
        let labels: Vec<(String, String)> = choices
            .iter()
            .map(|choice| (choice.clone(), String::new()))
            .collect();
        let default_index = default
            .and_then(|default| choices.iter().position(|choice| choice == default))
            .unwrap_or(0);
        let index = prompt_choice(&labels, default_index)?;
        return Ok(Some(choices[index].clone()));
    }
    let display = match default {
        // never echo a secret's default value in the prompt
        // (a site override may have replaced it with a real secret)
        Some(_) if secret => format!("{prompt} [default set]"),
        Some(default) => format!("{prompt} [{default}]"),
        None => prompt.to_owned(),
    };
    let value = prompt_value(&display, secret)?;
    Ok(value.or_else(|| default.map(str::to_owned)))
}

/// Prompt for a value; empty input yields None.
/// Secrets are read without echo when a terminal is available.
fn prompt_value(prompt: &str, secret: bool) -> anyhow::Result<Option<String>> {
    let value = if secret && std::io::stdin().is_terminal() {
        rpassword::prompt_password(format!("{prompt}: ")).context("Failed to read input")?
    } else {
        prompt_line(prompt)?
    };
    Ok(Some(value).filter(|value| !value.is_empty()))
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    eprint!("{prompt}: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let read = std::io::stdin()
        .read_line(&mut line)
        .context("Failed to read input")?;
    if read == 0 {
        bail!("Input closed before the wizard was complete");
    }
    Ok(line.trim().to_owned())
}
