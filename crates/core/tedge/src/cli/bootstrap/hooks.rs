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

/// The exit code by which a hook declares itself not applicable
const EXIT_SKIPPED: i32 = 2;

/// The environment variables exported to hooks, under the names
/// the `tedge` CLI itself reads: a hook runs `tedge config get ...`
/// as the operator would, without forwarding any flag
pub mod env {
    pub const CONFIG_DIR: &str = "TEDGE_CONFIG_DIR";
    pub const CLOUD_PROFILE: &str = "TEDGE_CLOUD_PROFILE";
    /// The config override for `device.id`: exported when the run was
    /// given a device id, and honoured as a `--device-id` substitute
    pub const DEVICE_ID: &str = "TEDGE_DEVICE_ID";
}

pub struct HookContext<'a> {
    pub config_dir: &'a Utf8Path,
    /// The layered hook roots (`bootstrap.plugin_paths`),
    /// earlier entries taking precedence per filename
    pub plugin_paths: &'a [Utf8PathBuf],
    pub cloud: &'a str,
    /// The target cloud URL as *intent*, passed as `--url <url>` when known
    /// (from a flag, the wizard, or a descriptor default that is not asked for):
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
    /// (e.g. registration inputs collected by the interactive wizard),
    /// on top of the run context exported by [`HookContext::context_envs`]
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
    /// current phase; skipped hooks are silent
    pub ui: &'a Ui,
    pub dry_run: bool,
}

impl HookContext<'_> {
    /// The arguments every hook of a phase receives
    fn args(&self, phase: Phase) -> Vec<String> {
        let mut args = vec![
            phase.as_str().to_owned(),
            "--cloud".to_owned(),
            self.cloud.to_owned(),
        ];
        if let Some(url) = &self.url {
            args.extend(["--url".to_owned(), url.clone()]);
        }
        if self.re_register || self.clean {
            args.push("--re-register".to_owned());
        }
        if self.clean {
            args.push("--clean".to_owned());
        }
        if self.offline {
            args.push("--offline".to_owned());
        }
        if let Some(cloud_type) = &self.cloud_type {
            args.extend(["--cloud-type".to_owned(), cloud_type.clone()]);
        }
        if let (Phase::Register, Some(method)) = (phase, &self.register_method) {
            args.extend(["--register-method".to_owned(), method.clone()]);
        }
        args
    }

    /// The context of the run, exported under the names the `tedge` CLI
    /// itself reads.
    ///
    /// These are not passed as flags: a hook reads the variable to build a
    /// path, and simply calls `tedge config get c8y.url` without forwarding
    /// anything, since the CLI takes the config directory and the profile
    /// from the environment. A hook that does pass `--config-dir` or
    /// `--profile` explicitly still overrides them.
    ///
    /// Safe to display: unlike the inputs collected for the run,
    /// none of it is secret.
    fn context_envs(&self) -> Vec<(String, String)> {
        let mut envs = vec![(env::CONFIG_DIR.to_owned(), self.config_dir.to_string())];
        if let Some(profile) = &self.profile {
            envs.push((env::CLOUD_PROFILE.to_owned(), profile.clone()));
        }
        envs
    }

    /// The environment every hook of a phase receives:
    /// the inputs collected for this run, then the run context
    /// (exported last, so an input cannot shadow the context)
    fn envs(&self) -> Vec<(String, String)> {
        let mut envs = self.envs.clone();
        envs.extend(self.context_envs());
        envs
    }
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
    let listing = list_hooks(phase, ctx.plugin_paths).await?;
    for path in &listing.not_executable {
        ctx.ui.line(&format!(
            "Warning: ignoring non-executable bootstrap hook {path}"
        ));
    }
    let args = ctx.args(phase);
    let envs = ctx.envs();
    // The dry-run trace is a command line the operator can replay: the
    // context is exported, not passed, so it is shown as a variable prefix.
    // Only the context is shown; the collected inputs may hold secrets
    let exported = ctx
        .context_envs()
        .iter()
        .map(|(env, value)| format!("{env}={value} "))
        .collect::<String>();
    for hook in &listing.hooks {
        let name = hook.file_name().unwrap_or_default();
        if ctx.dry_run {
            ctx.ui.line(&format!(
                "would run hook: {exported}{hook} {}",
                args.join(" ")
            ));
            continue;
        }
        ctx.ui.debug(&format!("running hook: {name}"));
        let mut command = tokio::process::Command::new(hook);
        command
            .args(&args)
            .envs(envs.iter().map(|(env, value)| (env, value)))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to execute bootstrap hook {hook}"))?;
        let (status, diagnostics) = stream_output(&mut child, ctx.ui)
            .await
            .with_context(|| format!("Failed to execute bootstrap hook {hook}"))?;
        match status.code() {
            Some(0) => {}
            Some(EXIT_SKIPPED) => ctx
                .ui
                .debug(&format!("hook skipped (not applicable): {name}")),
            code => {
                let code = match code {
                    Some(code) => code.to_string(),
                    None => "terminated by a signal".to_owned(),
                };
                ctx.ui
                    .fail_line(&format!("hook {name} failed (exit code: {code})"));
                for line in &diagnostics {
                    ctx.ui.replay_line(line);
                }
                bail!("Bootstrap hook {hook} failed (exit code: {code})")
            }
        }
    }
    Ok(listing.hooks.len())
}

/// How long a hook's output is still read after the hook exited:
/// a background process it left holding the pipes does not stall the run
const OUTPUT_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// Forward a hook's output until it exits: stdout is operator-facing
/// (shown, indented), stderr is diagnostics (logged; returned so a
/// failing hook's output can be replayed)
async fn stream_output(
    child: &mut tokio::process::Child,
    ui: &Ui,
) -> std::io::Result<(std::process::ExitStatus, Vec<String>)> {
    let mut stdout = child.stdout.take().map(tokio::io::BufReader::new);
    let mut stderr = child.stderr.take().map(tokio::io::BufReader::new);
    let mut diagnostics = Vec::new();
    let mut status = None;
    let grace = tokio::time::sleep(OUTPUT_GRACE);
    tokio::pin!(grace);
    loop {
        tokio::select! {
            line = read_line(&mut stdout), if stdout.is_some() => match line? {
                Some(line) => ui.line(&line),
                None => stdout = None,
            },
            line = read_line(&mut stderr), if stderr.is_some() => match line? {
                Some(line) => {
                    ui.debug(&line);
                    diagnostics.push(line);
                }
                None => stderr = None,
            },
            exited = child.wait(), if status.is_none() => {
                status = Some(exited?);
                grace.as_mut().reset(tokio::time::Instant::now() + OUTPUT_GRACE);
            }
            _ = &mut grace, if status.is_some() => {
                ui.debug("hook output left open by a background process, no longer read");
                break;
            }
            else => break,
        }
    }
    let status = match status {
        Some(status) => status,
        None => child.wait().await?,
    };
    Ok((status, diagnostics))
}

/// The next line of a hook's output, lossily decoded; `None` at EOF
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut Option<R>,
) -> std::io::Result<Option<String>> {
    use tokio::io::AsyncBufReadExt;
    let Some(reader) = reader else {
        return Ok(None);
    };
    let mut buf = Vec::new();
    if reader.read_until(b'\n', &mut buf).await? == 0 {
        return Ok(None);
    }
    let mut line = String::from_utf8_lossy(&buf).into_owned();
    while line.ends_with(['\n', '\r']) {
        line.pop();
    }
    Ok(Some(line))
}

/// The hooks of a phase, resolved across all layers
#[derive(Debug, Default, PartialEq, Eq)]
struct HookListing {
    /// The hooks to run, in lexical filename order
    hooks: Vec<Utf8PathBuf>,
    /// Files skipped because they are not executable
    not_executable: Vec<Utf8PathBuf>,
}

/// Resolve the hooks of a phase across all layers.
///
/// Earlier layers take precedence per filename
/// (the convention of `log.plugin_paths` and `configuration.plugin_paths`);
/// a file `<name>.ignore` in a layer disables the hook `<name>`
/// from that and all later layers.
async fn list_hooks(phase: Phase, plugin_paths: &[Utf8PathBuf]) -> anyhow::Result<HookListing> {
    // filename -> Some(path) to run, or None when disabled by a .ignore file.
    // Each layer decides its own names first (markers applied after the
    // regular files, so a marker always disables its sibling hook),
    // then the first layer to decide a name wins.
    let mut hooks: BTreeMap<String, Option<Utf8PathBuf>> = BTreeMap::new();
    let mut not_executable = Vec::new();
    for dir in phase_dirs(phase, plugin_paths) {
        let Some(mut entries) = read_dir_if_present(&dir).await? else {
            continue;
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
            // following symlinks: a hook may be linked in from a package
            let Ok(metadata) = tokio::fs::metadata(&path).await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if metadata.permissions().mode() & 0o111 == 0 {
                not_executable.push(path);
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
    Ok(HookListing {
        hooks: hooks.into_values().flatten().collect(),
        not_executable,
    })
}

/// The entries of a drop-in directory; `None` when there is no such
/// directory, an error when it exists but cannot be read
pub async fn read_dir_if_present(dir: &Utf8Path) -> anyhow::Result<Option<tokio::fs::ReadDir>> {
    match tokio::fs::read_dir(dir).await {
        Ok(entries) => Ok(Some(entries)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("Failed to read {dir}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_hook(dir: &Utf8Path, name: &str, script: &str) -> Utf8PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn write_noop_hook(dir: &Utf8Path, name: &str, executable: bool) -> Utf8PathBuf {
        let path = write_hook(dir, name, "exit 0");
        if !executable {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o644);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        path
    }

    fn test_ui(dir: &Utf8Path) -> Ui {
        let ui = Ui::new(Some(dir.as_std_path().to_owned()), true);
        ui.begin("test");
        ui
    }

    /// Where [`test_ui`] writes its log: the console output is also logged,
    /// so the log is how a test reads back what was displayed
    fn log_path(dir: &Utf8Path) -> Utf8PathBuf {
        dir.join(format!("tedge-bootstrap-{}.log", std::process::id()))
    }

    fn context<'a>(
        root: &'a Utf8Path,
        plugin_paths: &'a [Utf8PathBuf],
        ui: &'a Ui,
    ) -> HookContext<'a> {
        HookContext {
            config_dir: root,
            plugin_paths,
            cloud: "acme",
            url: Some("acme.example.com".into()),
            cloud_type: Some("c8y".into()),
            profile: Some("prod".into()),
            register_method: Some("token".into()),
            envs: vec![("ACME_TOKEN".into(), "t0k3n".into())],
            re_register: false,
            clean: true,
            offline: false,
            ui,
            dry_run: false,
        }
    }

    #[tokio::test]
    async fn site_layer_overrides_and_disables_hooks() {
        // Verify ordering, .ignore and executability rules in one layer
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap().join("bootstrap.d");
        let phase_dir = root.join("register.d");
        std::fs::create_dir_all(&phase_dir).unwrap();

        let b = write_noop_hook(&phase_dir, "10_b", true);
        let a = write_noop_hook(&phase_dir, "05_a", true);
        write_noop_hook(&phase_dir, "20_disabled", true);
        write_noop_hook(&phase_dir, "20_disabled.ignore", true);
        let not_executable = write_noop_hook(&phase_dir, "30_not_executable", false);

        let listing = list_hooks(Phase::Register, &[root]).await.unwrap();
        assert_eq!(listing.hooks, vec![a, b]);
        assert_eq!(listing.not_executable, vec![not_executable]);
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

        let overriding = write_noop_hook(&site.join("register.d"), "10_hook", true);
        write_noop_hook(&site.join("register.d"), "20_disabled_by_site.ignore", true);
        let site_only = write_noop_hook(&site.join("register.d"), "30_site_only", true);
        write_noop_hook(&packaged.join("register.d"), "10_hook", true);
        write_noop_hook(&packaged.join("register.d"), "20_disabled_by_site", true);
        let packaged_only = write_noop_hook(&packaged.join("register.d"), "40_packaged_only", true);

        let listing = list_hooks(Phase::Register, &[site, packaged])
            .await
            .unwrap();
        assert_eq!(listing.hooks, vec![overriding, site_only, packaged_only]);
    }

    #[tokio::test]
    async fn hooks_receive_the_documented_arguments_and_environment() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let plugin_paths = vec![root.join("bootstrap.d")];
        let phase_dir = plugin_paths[0].join("register.d");
        std::fs::create_dir_all(&phase_dir).unwrap();
        let record = root.join("record");
        write_hook(
            &phase_dir,
            "10_record",
            &format!(
                "printf '%s\\n' \"$@\" \"config_dir=$TEDGE_CONFIG_DIR\" \
                 \"profile=$TEDGE_CLOUD_PROFILE\" \"token=$ACME_TOKEN\" > {record}"
            ),
        );
        let ui = test_ui(root);
        let ctx = context(root, &plugin_paths, &ui);

        let run = run_phase(Phase::Register, &ctx).await.unwrap();
        assert_eq!(run, 1);
        let recorded = std::fs::read_to_string(&record).unwrap();
        let expected = [
            "register",
            "--cloud",
            "acme",
            "--url",
            "acme.example.com",
            // --clean implies --re-register
            "--re-register",
            "--clean",
            "--cloud-type",
            "c8y",
            "--register-method",
            "token",
            // the config directory and the profile are exported, not passed:
            // a hook runs `tedge config get` without forwarding any flag
            &format!("config_dir={root}"),
            "profile=prod",
            "token=t0k3n",
        ]
        .join("\n");
        assert_eq!(recorded.trim_end(), expected);

        // the register method is only passed to register hooks
        assert!(!ctx
            .args(Phase::Prepare)
            .contains(&"--register-method".to_owned()));
    }

    #[tokio::test]
    async fn skipped_hooks_count_as_run_and_failing_hooks_abort_the_phase() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let plugin_paths = vec![root.join("bootstrap.d")];
        let phase_dir = plugin_paths[0].join("finalize.d");
        std::fs::create_dir_all(&phase_dir).unwrap();
        write_hook(&phase_dir, "10_skipped", "exit 2");
        write_hook(&phase_dir, "20_done", "echo done");
        let ui = test_ui(root);
        let ctx = context(root, &plugin_paths, &ui);
        assert_eq!(run_phase(Phase::Finalize, &ctx).await.unwrap(), 2);

        write_hook(
            &phase_dir,
            "30_failing",
            "echo 'something went wrong' >&2; exit 3",
        );
        let touched = root.join("never");
        write_hook(&phase_dir, "40_never_reached", &format!("touch {touched}"));
        let err = run_phase(Phase::Finalize, &ctx).await.unwrap_err();
        assert!(err.to_string().contains("exit code: 3"), "{err}");
        assert!(!touched.exists());
    }

    #[tokio::test]
    async fn dry_runs_list_the_hooks_without_executing_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let plugin_paths = vec![root.join("bootstrap.d")];
        let phase_dir = plugin_paths[0].join("prepare.d");
        std::fs::create_dir_all(&phase_dir).unwrap();
        let touched = root.join("touched");
        let hook = write_hook(&phase_dir, "10_touch", &format!("touch {touched}"));
        let ui = test_ui(root);
        let ctx = HookContext {
            dry_run: true,
            ..context(root, &plugin_paths, &ui)
        };
        assert_eq!(run_phase(Phase::Prepare, &ctx).await.unwrap(), 1);
        assert!(!touched.exists());

        // the trace is a command line the operator can replay:
        // the exported context is shown as a variable prefix,
        // and the collected inputs, which may be secret, are not shown
        let trace = std::fs::read_to_string(log_path(root)).unwrap();
        assert!(
            trace.contains(&format!(
                "would run hook: TEDGE_CONFIG_DIR={root} TEDGE_CLOUD_PROFILE=prod {hook} prepare"
            )),
            "{trace}"
        );
        assert!(!trace.contains("t0k3n"), "{trace}");
    }

    #[tokio::test]
    async fn missing_hook_directories_are_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let plugin_paths = vec![root.join("does-not-exist")];
        let ui = test_ui(root);
        let ctx = context(root, &plugin_paths, &ui);
        assert_eq!(run_phase(Phase::Configure, &ctx).await.unwrap(), 0);
    }
}
