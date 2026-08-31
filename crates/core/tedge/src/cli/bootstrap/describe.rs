//! Live documentation of the resolved cloud descriptors (`--describe`)
//!
//! Rendered from the same descriptors that drive the wizard,
//! validation, and error messages - packaged clouds and clouds.d
//! overrides included - so the output is exactly what this device
//! would ask for, and third-party clouds document themselves.

use super::descriptor::CloudDescriptor;
use super::descriptor::MethodInput;
use super::descriptor::Setting;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use std::fmt::Write;
use tedge_config::TEdgeConfig;

pub struct DescribeCommand {
    pub output: String,
}

#[async_trait::async_trait]
impl Command for DescribeCommand {
    fn description(&self) -> String {
        "describe the available clouds and their bootstrap options".to_owned()
    }

    async fn execute(&self, _config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        print!("{}", self.output);
        Ok(())
    }
}

/// Render the descriptors; a cloud key narrows the output to that cloud.
///
/// The listing skips hidden clouds (clouds.d ignore markers);
/// naming a hidden cloud explicitly still describes it, with a note.
pub fn render(descriptors: &[CloudDescriptor], cloud: Option<&str>) -> anyhow::Result<String> {
    let selected: Vec<&CloudDescriptor> = match cloud {
        Some(name) => {
            let descriptor = descriptors
                .iter()
                .find(|d| d.cloud == name)
                .ok_or_else(|| {
                    let available = descriptors
                        .iter()
                        .filter(|d| !d.hidden)
                        .map(|d| d.cloud.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    anyhow!(
                        "No cloud descriptor found for {name:?}; available: {available}. \
                     A custom mapper without a descriptor delegates registration \
                     to the register.d hooks"
                    )
                })?;
            vec![descriptor]
        }
        None => descriptors.iter().filter(|d| !d.hidden).collect(),
    };
    let mut out = String::new();
    for (i, descriptor) in selected.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_cloud(&mut out, descriptor);
    }
    Ok(out)
}

fn render_cloud(out: &mut String, descriptor: &CloudDescriptor) {
    let _ = writeln!(out, "{} ({})", descriptor.description, descriptor.cloud);
    if descriptor.hidden {
        let _ = writeln!(
            out,
            "  hidden: not offered by the wizard (clouds.d ignore marker)"
        );
    }
    if let Some(cloud_type) = &descriptor.cloud_type {
        let _ = writeln!(out, "  type: {cloud_type}");
    }

    let url = match &descriptor.url {
        Some(spec) if spec.fixed_value().is_some() => {
            format!("{} (fixed)", spec.fixed_value().unwrap())
        }
        Some(spec) => {
            let mut line = if spec.description.is_empty() {
                format!("{} URL", descriptor.description)
            } else {
                spec.description.clone()
            };
            if let Some(default) = &spec.default {
                let _ = write!(line, " (default: {default})");
            }
            if !spec.choices.is_empty() {
                let _ = write!(line, " (choices: {})", spec.choices.join(", "));
            }
            line
        }
        None => format!("{} URL", descriptor.description),
    };
    let _ = writeln!(out, "  url: {url}");

    let _ = writeln!(out, "\n  registration methods:");
    if descriptor.register.is_empty() {
        let _ = writeln!(out, "    none declared (fulfilled by the register.d hooks)");
    }
    for method in &descriptor.register {
        let marker = if method.default { " (default)" } else { "" };
        let description = match method.description.is_empty() {
            true => String::new(),
            false => format!(": {}", method.description),
        };
        let _ = writeln!(out, "    {}{marker}{description}", method.name);
        for input in &method.inputs {
            render_input(out, input);
        }
    }

    if descriptor.settings.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n  settings:");
    for setting in &descriptor.settings {
        render_setting(out, descriptor, setting);
    }
}

fn render_input(out: &mut String, input: &MethodInput) {
    let mut notes = Vec::new();
    if input.secret {
        notes.push("secret".to_owned());
    }
    if !input.is_required() {
        notes.push("optional".to_owned());
    }
    if let Some(default) = &input.default {
        notes.push(format!("default: {default}"));
    }
    if !input.choices.is_empty() {
        notes.push(format!("choices: {}", input.choices.join(", ")));
    }
    let notes = match notes.is_empty() {
        true => String::new(),
        false => format!("  ({})", notes.join(", ")),
    };
    let _ = writeln!(out, "      - {}  ${}{notes}", input.name, input.env);
    if !input.description.is_empty() {
        let _ = writeln!(out, "        {}", input.description);
    }
}

fn render_setting(out: &mut String, descriptor: &CloudDescriptor, setting: &Setting) {
    let key = match setting.global {
        true => format!("{} (global)", setting.key),
        false => format!("{}.{}", descriptor.cloud, setting.key),
    };
    let title = setting
        .name
        .clone()
        .unwrap_or_else(|| setting.description.clone());
    let mut notes = Vec::new();
    if setting.required {
        notes.push("required".to_owned());
    }
    if let Some(default) = &setting.default {
        notes.push(format!("default: {default}"));
    }
    let notes = match notes.is_empty() {
        true => String::new(),
        false => format!(" ({})", notes.join(", ")),
    };
    match title.is_empty() {
        true => {
            let _ = writeln!(out, "    {key}{notes}");
        }
        false => {
            let _ = writeln!(out, "    {key}: {title}{notes}");
        }
    }
    for choice in &setting.choices {
        let label = match choice.label() == choice.value() {
            true => choice.value().to_owned(),
            false => format!("{}: {}", choice.value(), choice.label()),
        };
        let description = match choice.description().is_empty() {
            true => String::new(),
            false => format!(" - {}", choice.description()),
        };
        let _ = writeln!(out, "      - {label}{description}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::bootstrap::descriptor::builtin_descriptors;

    #[test]
    fn describes_the_builtin_clouds() {
        let rendered = render(&builtin_descriptors(), None).unwrap();
        // methods carry their inputs as environment variable names
        assert!(rendered.contains("c8y-ca (default)"), "{rendered}");
        assert!(
            rendered.contains("one-time password  $DEVICE_ONE_TIME_PASSWORD  (secret, optional)"),
            "{rendered}"
        );
        assert!(rendered.contains("$C8Y_USER"), "{rendered}");
        // settings show the full default-instance key and their choices
        assert!(rendered.contains("c8y.mqtt_service.enabled"), "{rendered}");
        assert!(rendered.contains("true: MQTT Service"), "{rendered}");
        // every built-in cloud is present
        for cloud in ["c8y", "az", "aws"] {
            assert!(render(&builtin_descriptors(), Some(cloud)).is_ok());
        }
    }

    #[test]
    fn unknown_clouds_are_rejected_with_the_available_list() {
        let err = render(&builtin_descriptors(), Some("nope")).unwrap_err();
        assert!(err.to_string().contains("available:"), "{err}");
    }
}
