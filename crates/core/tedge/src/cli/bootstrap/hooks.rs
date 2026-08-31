use super::ui::Ui;
use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

/// The named extension points of the bootstrap pipeline.
///
/// Each phase maps to drop-in directories `<layer>/<phase>.d/`
/// holding ordered executables, following the diagnostic-plugin conventions:
/// lexical ordering, a `.ignore` suffix disables a hook,
/// exit code 0 = done, 2 = skipped / not applicable, anything else aborts.
///
/// The layers are the directories configured as `bootstrap.plugin_paths`,
/// by default `<config-dir>/bootstrap.d/<phase>.d/` (site customization)
/// before `/usr/share/tedge/bootstrap.d/<phase>.d/` (shipped by packages).
/// Earlier directories take precedence per filename
/// (matching `log.plugin_paths` and `configuration.plugin_paths`):
/// a site hook overrides a packaged hook with the same filename,
/// and a site file `<name>.ignore` disables the packaged hook `<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Before anything is resolved or written
    /// (named after the software management plugin API's `prepare`,
    /// pairing with `finalize`)
    Prepare,
    /// After the endpoints are resolved and the configuration is written
    Configure,
    Register,
    Finalize,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Configure => "configure",
            Self::Register => "register",
            Self::Finalize => "finalize",
        }
    }
}

pub struct HookContext<'a> {
    pub config_dir: &'a Utf8Path,
    /// The layered hook roots (`bootstrap.plugin_paths`),
    /// earlier entries taking precedence per filename
    pub plugin_paths: &'a [Utf8PathBuf],
    pub cloud: &'a str,
    /// The target cloud URL as *intent*, passed as `--url <url>` when known
    /// (from a flag, the wizard, or a fixed descriptor value):
    /// prepare-phase hooks run before it is persisted,
    /// so they cannot read it from the config yet.
    /// On re-runs where the URL is only in the existing configuration,
    /// hooks read it with `tedge config get` instead
    pub url: Option<String>,
    /// The declared cloud type of a custom-named instance,
    /// passed to hooks as `--cloud-type <type>` so a hook serving
    /// e.g. c8y semantics can also self-select on typed instances
    pub cloud_type: Option<String>,
    pub profile: Option<String>,
    /// The chosen registration method, passed to register-phase hooks
    /// as `--register-method <name>` so one cloud's hook can offer
    /// several methods (e.g. token vs certificate)
    pub register_method: Option<String>,
    /// Extra environment variables set on hook processes
    /// (e.g. registration inputs collected by the interactive wizard)
    pub envs: Vec<(String, String)>,
    /// A re-registration run: hooks receive `--re-register`
    /// (also on a `--clean` run, which implies it)
    /// so they can drop their idempotency guards and re-register
    pub re_register: bool,
    /// A full unwind: hooks additionally receive `--clean`
    /// so they can remove their own state too
    pub clean: bool,
    /// Offline provisioning: hooks receive `--offline` so they can
    /// self-adapt (a connectivity preflight skips, a local-PKI register
    /// hook may still fulfil registration)
    pub offline: bool,
    /// Structured console output: hook output is indented under the
    /// current phase; skipped hooks are silent unless --verbose
    pub ui: &'a Ui,
    pub dry_run: bool,
}

/// The directories searched for hooks of the given phase, in layering order
pub fn phase_dirs(phase: Phase, plugin_paths: &[Utf8PathBuf]) -> Vec<Utf8PathBuf> {
    let phase_dir = format!("{}.d", phase.as_str());
    plugin_paths
        .iter()
        .map(|root| root.join(&phase_dir))
        .collect()
}

/// Run all hooks of the given phase, in lexical filename order.
///
/// Returns the number of hooks run (or, on dry-run, that would run).
/// Missing or empty hook directories are not an error.
pub async fn run_phase(phase: Phase, ctx: &HookContext<'_>) -> anyhow::Result<usize> {
    let hooks = list_hooks(phase, ctx.plugin_paths).await?;
    for hook in &hooks {
        let name = hook.file_name().unwrap_or_default();
        if ctx.dry_run {
            let url = match &ctx.url {
                Some(url) => format!(" --url {url}"),
                None => String::new(),
            };
            let unwind = match (ctx.re_register || ctx.clean, ctx.clean) {
                (_, true) => " --re-register --clean",
                (true, false) => " --re-register",
                (false, false) => "",
            };
            let offline = if ctx.offline { " --offline" } else { "" };
            let method = match (&phase, &ctx.register_method) {
                (Phase::Register, Some(method)) => format!(" --register-method {method}"),
                _ => String::new(),
            };
            ctx.ui.line(&format!(
                "would run hook: {hook} {} --cloud {} --config-dir {}{url}{unwind}{offline}{method}",
                phase.as_str(),
                ctx.cloud,
                ctx.config_dir
            ));
            continue;
        }
        ctx.ui.debug(&format!("running hook: {name}"));
        let mut command = tokio::process::Command::new(hook);
        command
            .arg(phase.as_str())
            .arg("--cloud")
            .arg(ctx.cloud)
            .arg("--config-dir")
            .arg(ctx.config_dir);
        if let Some(url) = &ctx.url {
            command.arg("--url").arg(url);
        }
        if ctx.re_register || ctx.clean {
            command.arg("--re-register");
        }
        if ctx.clean {
            command.arg("--clean");
        }
        if ctx.offline {
            command.arg("--offline");
        }
        if let Some(cloud_type) = &ctx.cloud_type {
            command.arg("--cloud-type").arg(cloud_type);
        }
        if let Some(profile) = &ctx.profile {
            command.arg("--profile").arg(profile);
        }
        if phase == Phase::Register {
            if let Some(method) = &ctx.register_method {
                command.arg("--register-method").arg(method);
            }
        }
        for (env, value) in &ctx.envs {
            command.env(env, value);
        }
        // Hook output is streamed line by line, indented under the phase
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to execute bootstrap hook {hook}"))?;
        let diagnostics = stream_output(&mut child, ctx.ui).await?;
        let status = child
            .wait()
            .await
            .with_context(|| format!("Failed to execute bootstrap hook {hook}"))?;
        match status.code() {
            Some(0) => {}
            Some(2) => ctx
                .ui
                .debug(&format!("hook skipped (not applicable): {name}")),
            code => {
                let code = match code {
                    Some(code) => code.to_string(),
                    None => "terminated by a signal".to_owned(),
                };
                ctx.ui
                    .fail_line(&format!("hook {name} failed (exit code: {code})"));
                if !ctx.ui.verbose {
                    for line in &diagnostics {
                        ctx.ui.replay_line(line);
                    }
                }
                bail!("Bootstrap hook {hook} failed (exit code: {code})")
            }
        }
    }
    Ok(hooks.len())
}

/// Forward a hook's output: stdout is operator-facing (shown, indented),
/// stderr is diagnostics (logged; returned so a failing hook's output
/// can be replayed)
async fn stream_output(child: &mut tokio::process::Child, ui: &Ui) -> anyhow::Result<Vec<String>> {
    use tokio::io::AsyncBufReadExt;
    let mut stdout = child
        .stdout
        .take()
        .map(|stream| tokio::io::BufReader::new(stream).lines());
    let mut stderr = child
        .stderr
        .take()
        .map(|stream| tokio::io::BufReader::new(stream).lines());
    let mut diagnostics = Vec::new();
    loop {
        tokio::select! {
            line = async {
                match &mut stdout {
                    Some(lines) => lines.next_line().await,
                    None => Ok(None),
                }
            }, if stdout.is_some() => match line? {
                Some(line) => ui.line(&line),
                None => stdout = None,
            },
            line = async {
                match &mut stderr {
                    Some(lines) => lines.next_line().await,
                    None => Ok(None),
                }
            }, if stderr.is_some() => match line? {
                Some(line) => {
                    ui.debug(&line);
                    diagnostics.push(line);
                }
                None => stderr = None,
            },
            else => break,
        }
    }
    Ok(diagnostics)
}

/// Resolve the hooks of a phase across all layers.
///
/// Earlier layers take precedence per filename
/// (the convention of `log.plugin_paths` and `configuration.plugin_paths`);
/// a file `<name>.ignore` in a layer disables the hook `<name>`
/// from that and all later layers.
async fn list_hooks(
    phase: Phase,
    plugin_paths: &[Utf8PathBuf],
) -> anyhow::Result<Vec<Utf8PathBuf>> {
    // filename -> Some(path) to run, or None when disabled by a .ignore file.
    // Each layer decides its own names first (markers applied after the
    // regular files, so a marker always disables its sibling hook),
    // then the first layer to decide a name wins.
    let mut hooks: BTreeMap<String, Option<Utf8PathBuf>> = BTreeMap::new();
    for dir in phase_dirs(phase, plugin_paths) {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        let mut layer: BTreeMap<String, Option<Utf8PathBuf>> = BTreeMap::new();
        let mut disabled_in_layer = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let Ok(path) = Utf8PathBuf::try_from(entry.path()) else {
                continue;
            };
            let Some(name) = path.file_name().map(|n| n.to_owned()) else {
                continue;
            };
            if let Some(disabled) = name.strip_suffix(".ignore") {
                disabled_in_layer.push(disabled.to_owned());
                continue;
            }
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if metadata.permissions().mode() & 0o111 == 0 {
                eprintln!("Warning: ignoring non-executable bootstrap hook {path}");
                continue;
            }
            layer.insert(name, Some(path));
        }
        for disabled in disabled_in_layer {
            layer.insert(disabled, None);
        }
        for (name, path) in layer {
            hooks.entry(name).or_insert(path);
        }
    }
    Ok(hooks.into_values().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_hook(dir: &Utf8Path, name: &str, executable: bool) -> Utf8PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        if executable {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn site_layer_overrides_and_disables_hooks() {
        // Verify ordering, .ignore and executability rules in one layer
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap().join("bootstrap.d");
        let phase_dir = root.join("register.d");
        std::fs::create_dir_all(&phase_dir).unwrap();

        let b = write_hook(&phase_dir, "10_b", true);
        let a = write_hook(&phase_dir, "05_a", true);
        write_hook(&phase_dir, "20_disabled", true);
        write_hook(&phase_dir, "20_disabled.ignore", true);
        write_hook(&phase_dir, "30_not_executable", false);

        let hooks = list_hooks(Phase::Register, &[root]).await.unwrap();
        assert_eq!(hooks, vec![a, b]);
    }

    #[tokio::test]
    async fn earlier_plugin_paths_override_and_disable_later_ones() {
        // site customization comes first in bootstrap.plugin_paths and wins,
        // matching the log.plugin_paths / configuration.plugin_paths convention
        let tmp = tempfile::tempdir().unwrap();
        let base = Utf8Path::from_path(tmp.path()).unwrap();
        let site = base.join("site/bootstrap.d");
        let packaged = base.join("packaged/bootstrap.d");
        std::fs::create_dir_all(site.join("register.d")).unwrap();
        std::fs::create_dir_all(packaged.join("register.d")).unwrap();

        let overriding = write_hook(&site.join("register.d"), "10_hook", true);
        write_hook(&site.join("register.d"), "20_disabled_by_site.ignore", true);
        let site_only = write_hook(&site.join("register.d"), "30_site_only", true);
        write_hook(&packaged.join("register.d"), "10_hook", true);
        write_hook(&packaged.join("register.d"), "20_disabled_by_site", true);
        let packaged_only = write_hook(&packaged.join("register.d"), "40_packaged_only", true);

        let hooks = list_hooks(Phase::Register, &[site, packaged])
            .await
            .unwrap();
        assert_eq!(hooks, vec![overriding, site_only, packaged_only]);
    }
}
