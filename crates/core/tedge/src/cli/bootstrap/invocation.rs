//! Captured bootstrap invocations
//!
//! An invocation file is a **JSON array** of invocations —
//! an array even for a single instance, so appending another
//! (e.g. a second Cumulocity instance) is natural.
//! `--save` writes it, `--from` replays it in order.

use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use std::collections::BTreeMap;

/// The effective invocation of one bootstrap run:
/// the wizard's "equivalent command" as a declarative document.
///
/// The fields are the CLI flags themselves.
/// Environment variables are listed by *name* only — secret values are
/// never captured — so a saved invocation can be replayed with
/// `tedge bootstrap --from <file>` given the same environment.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Invocation {
    pub cloud: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub cloud_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, String>,
    /// Names of the environment variables the run was given
    /// (registration inputs); values are never captured
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    /// Re-run registration: drop the registration artifacts first
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub re_register: bool,
    /// Full unwind: drop the artifacts and the instance's configuration
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clean: bool,
}

impl Invocation {
    /// Render the invocation as the equivalent non-interactive command.
    ///
    /// This is what an interactive run prints before it starts,
    /// so the wizard teaches the automatable form of what it is about to do
    /// rather than diverging from it.
    pub fn command_line(&self) -> String {
        let mut command = format!("tedge bootstrap {}", self.cloud);
        let mut push = |flag: &str, value: &str| command.push_str(&format!(" {flag} {value}"));
        if let Some(profile) = &self.profile {
            push("--profile", profile);
        }
        if let Some(cloud_type) = &self.cloud_type {
            push("--type", cloud_type);
        }
        if let Some(url) = &self.url {
            push("--url", url);
        }
        if let Some(register) = &self.register {
            push("--register", register);
        }
        if let Some(device_id) = &self.device_id {
            push("--device-id", device_id);
        }
        for (key, value) in &self.set {
            push("--set", &format!("{key}={value}"));
        }
        // --clean implies --re-register, so only the wider flag is shown
        if self.clean {
            command.push_str(" --clean");
        } else if self.re_register {
            command.push_str(" --re-register");
        }
        command
    }
}

/// Parse an invocation file: a JSON array of invocations, run in order
pub fn parse_invocations(content: &str) -> anyhow::Result<Vec<Invocation>> {
    let invocations: Vec<Invocation> = serde_json::from_str(content)
        .context("Expected a JSON array of bootstrap invocations (the format --save writes)")?;
    if invocations.is_empty() {
        bail!("The invocation file contains no invocations");
    }
    Ok(invocations)
}

/// Write the invocations as a declarative document (`--save`):
/// a pretty-printed JSON array, replayable with `--from`
pub async fn save_invocations(path: &Utf8Path, invocations: &[Invocation]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(invocations)?;
    tokio::fs::write(path, json + "\n")
        .await
        .with_context(|| format!("Failed to write {path}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocations_round_trip_as_an_array() {
        let invocation = Invocation {
            cloud: "c8y-second".into(),
            profile: None,
            cloud_type: Some("c8y".into()),
            url: Some("example.cumulocity.com".into()),
            register: Some("basic".into()),
            device_id: Some("demo01".into()),
            set: BTreeMap::from([("c8y-second.mqtt_service.enabled".into(), "true".into())]),
            env: vec!["C8Y_BOOTSTRAP_USER".into()],
            re_register: false,
            clean: false,
        };
        let json = serde_json::to_string_pretty(&[invocation]).unwrap();
        // secrets are captured by name only
        assert!(json.contains("C8Y_BOOTSTRAP_USER"));
        // appending a second instance is a plain array edit
        let appended = json.replacen(
            '[',
            r#"[{"cloud": "thingsboard", "url": "tb.example.com"},"#,
            1,
        );
        let parsed = parse_invocations(&appended).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].cloud, "thingsboard");
        assert_eq!(parsed[1].cloud, "c8y-second");
        assert_eq!(parsed[1].cloud_type.as_deref(), Some("c8y"));
    }

    #[test]
    fn an_invocation_renders_the_equivalent_command() {
        let invocation = Invocation {
            cloud: "c8y".into(),
            profile: Some("prod".into()),
            cloud_type: None,
            url: Some("example.cumulocity.com".into()),
            register: Some("c8y-ca".into()),
            device_id: Some("demo01".into()),
            set: BTreeMap::from([(
                "c8y.profiles.prod.mqtt_service.enabled".into(),
                "true".into(),
            )]),
            env: vec!["C8Y_BOOTSTRAP_USER".into()],
            re_register: true,
            clean: true,
        };
        assert_eq!(
            invocation.command_line(),
            "tedge bootstrap c8y --profile prod --url example.cumulocity.com \
             --register c8y-ca --device-id demo01 \
             --set c8y.profiles.prod.mqtt_service.enabled=true --clean"
        );
    }

    #[test]
    fn non_array_files_are_rejected_with_guidance() {
        let err = parse_invocations(r#"{"cloud": "c8y"}"#).unwrap_err();
        assert!(err.to_string().contains("JSON array"), "{err}");
        assert!(parse_invocations("[]").is_err());
        assert!(parse_invocations("not json").is_err());
    }
}
