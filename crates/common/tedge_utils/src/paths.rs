use std::ffi::OsString;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::file::FileError;
use crate::file::PermissionEntry;
use crate::fs::atomically_write_file_async;

const PERMISSION_BITS: u32 = 0o777;
const DEFAULT_UMASK: u32 = 0o022;
const DEFAULT_DIR_MODE: u32 = apply_umask(0o777);
const DEFAULT_FILE_MODE: u32 = apply_umask(0o666);
const GROUP_WRITE: u32 = 0o020;

const fn apply_umask(mode: u32) -> u32 {
    let mode = mode & PERMISSION_BITS;
    let umask = DEFAULT_UMASK & PERMISSION_BITS;

    mode & (PERMISSION_BITS ^ umask)
}

#[derive(thiserror::Error, Debug)]
pub enum PathsError {
    #[error("User's Home Directory not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Path conversion to String failed: {path:?}.")]
    PathToStringFailed { path: OsString },

    #[error("Directory: {path:?} not found")]
    DirNotFound { path: OsString },

    #[error("Parent directory for the path: {path:?} not found")]
    ParentDirNotFound { path: OsString },

    #[error("Relative path: {path:?} is not permitted. Provide an absolute path instead.")]
    RelativePathNotPermitted { path: OsString },

    #[error("Managed path {path:?} must stay relative to the config root")]
    InvalidManagedPath { path: PathBuf },

    #[error("Managed path {path:?} is outside the config root")]
    PathOutsideRoot { path: PathBuf },

    #[error(transparent)]
    FileError(#[from] crate::file::FileError),

    #[error(transparent)]
    AtomFileError(#[from] crate::fs::AtomFileError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub user: String,
    pub group: String,
}

impl Owner {
    pub fn user_group(user: impl Into<String>, group: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            group: group.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TedgePaths {
    root: PathBuf,
    default_owner: Owner,
}

impl TedgePaths {
    pub fn from_root_with_defaults(
        root: impl AsRef<Path>,
        user: impl Into<String>,
        group: impl Into<String>,
    ) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            default_owner: Owner::user_group(user, group),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_owner(&self) -> &Owner {
        &self.default_owner
    }

    pub fn root_dir(&self) -> ManagedDir {
        ManagedDir {
            root: self.root.clone(),
            path: self.root.clone(),
            owner: self.default_owner.clone(),
            mode: DEFAULT_DIR_MODE,
            warn_and_ignore_permission_errors: false,
        }
    }

    pub fn dir(&self, path: impl AsRef<Path>) -> Result<ManagedDir, PathsError> {
        Ok(ManagedDir {
            root: self.root.clone(),
            path: self.resolve(path)?,
            owner: self.default_owner.clone(),
            mode: DEFAULT_DIR_MODE,
            warn_and_ignore_permission_errors: false,
        })
    }

    pub fn file(&self, path: impl AsRef<Path>) -> Result<ManagedFile, PathsError> {
        Ok(ManagedFile {
            path: self.resolve(path)?,
            owner: self.default_owner.clone(),
            mode: DEFAULT_FILE_MODE,
            warn_and_ignore_permission_errors: false,
        })
    }

    pub fn template_file(&self, path: impl AsRef<Path>) -> Result<ManagedTemplateFile, PathsError> {
        let path = self.resolve(path)?;
        let parent = path.parent().map(|path| ManagedDir {
            root: self.root.clone(),
            path: path.to_owned(),
            owner: self.default_owner.clone(),
            mode: DEFAULT_DIR_MODE,
            warn_and_ignore_permission_errors: false,
        });

        Ok(ManagedTemplateFile {
            active: ManagedFile {
                path,
                owner: self.default_owner.clone(),
                mode: DEFAULT_FILE_MODE,
                warn_and_ignore_permission_errors: false,
            },
            parent,
            warn_and_ignore_permission_errors: false,
        })
    }

    fn resolve(&self, path: impl AsRef<Path>) -> Result<PathBuf, PathsError> {
        let path = path.as_ref();
        let relative_path = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .map_err(|_| PathsError::PathOutsideRoot {
                    path: path.to_path_buf(),
                })?
        } else {
            path
        };

        validate_managed_path(relative_path)?;
        Ok(self.root.join(relative_path))
    }
}

#[derive(Debug, Clone)]
pub struct ManagedDir {
    root: PathBuf,
    path: PathBuf,
    owner: Owner,
    mode: u32,
    warn_and_ignore_permission_errors: bool,
}

impl ManagedDir {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    pub fn with_owner(mut self, owner: Owner) -> Self {
        self.owner = owner;
        self
    }

    pub fn preserve_ownership(self) -> Self {
        self.with_owner(Owner::user_group("", ""))
    }

    pub fn group_writable(mut self) -> Self {
        self.mode |= GROUP_WRITE;
        self
    }

    pub fn warn_and_ignore_permission_errors(mut self) -> Self {
        self.warn_and_ignore_permission_errors = true;
        self
    }

    pub async fn ensure(&self) -> Result<(), PathsError> {
        if self.warn_and_ignore_permission_errors {
            self.ensure_without_strict_permissions().await
        } else {
            self.ensure_strict_permissions().await
        }
    }

    async fn ensure_strict_permissions(&self) -> Result<(), PathsError> {
        let permissions = self.permission_entry().force_dir_ownership();
        let result = permissions
            .create_directory_with_root(self.path(), &self.root)
            .await
            .map_err(Into::into);
        self.handle_permission_errors(result)
    }

    async fn ensure_without_strict_permissions(&self) -> Result<(), PathsError> {
        self.create_directory_tree_with_root(self.path()).await?;
        let result = self
            .permission_entry()
            .apply(self.path())
            .await
            .map_err(Into::into);
        self.handle_permission_errors(result)
    }

    async fn create_directory_tree_with_root(&self, dir: &Path) -> Result<(), FileError> {
        match dir.parent() {
            None => return Ok(()),
            Some(_parent) if dir == self.root => {}
            Some(parent) => {
                if !tokio::fs::try_exists(parent).await.unwrap_or(false) {
                    Box::pin(self.create_directory_tree_with_root(parent)).await?;
                }
            }
        }

        match tokio::fs::create_dir(dir).await {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(err) => Err(FileError::DirectoryCreateFailed {
                dir: dir.display().to_string(),
                from: err,
            }),
        }
    }

    pub async fn create_if_missing(&self) -> Result<(), PathsError> {
        let permissions = self.permission_entry();
        let result = permissions
            .create_directory(self.path())
            .await
            .map_err(Into::into);
        self.handle_permission_errors(result)
    }

    fn permission_entry(&self) -> PermissionEntry {
        PermissionEntry::new(
            non_empty_string(&self.owner.user),
            non_empty_string(&self.owner.group),
            Some(self.mode),
        )
    }

    fn handle_permission_errors(&self, result: Result<(), PathsError>) -> Result<(), PathsError> {
        if self.warn_and_ignore_permission_errors {
            ignore_owner_or_mode_error(result)
        } else {
            result
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagedFile {
    path: PathBuf,
    owner: Owner,
    mode: u32,
    warn_and_ignore_permission_errors: bool,
}

impl ManagedFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    pub fn with_owner(mut self, owner: Owner) -> Self {
        self.owner = owner;
        self
    }

    pub fn preserve_ownership(self) -> Self {
        self.with_owner(Owner::user_group("", ""))
    }

    pub fn warn_and_ignore_permission_errors(mut self) -> Self {
        self.warn_and_ignore_permission_errors = true;
        self
    }

    pub async fn replace_atomic(&self, content: impl AsRef<[u8]>) -> Result<(), PathsError> {
        let result = async {
            atomically_write_file_async(&self.path, content.as_ref()).await?;
            self.permission_entry().apply(&self.path).await?;
            Ok(())
        }
        .await;
        self.handle_permission_errors(result)
    }

    pub async fn create_if_missing(&self, content: impl AsRef<[u8]>) -> Result<(), PathsError> {
        let mut options = tokio::fs::OpenOptions::new();
        match options.create_new(true).write(true).open(&self.path).await {
            Ok(mut file) => {
                file.write_all(content.as_ref()).await?;
                file.flush().await?;
                file.sync_all().await?;
                let result = self
                    .permission_entry()
                    .apply(&self.path)
                    .await
                    .map_err(Into::into);
                self.handle_permission_errors(result)
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn ensure_permissions(&self) -> Result<(), PathsError> {
        let result = self
            .permission_entry()
            .apply(&self.path)
            .await
            .map_err(Into::into);
        self.handle_permission_errors(result)
    }

    fn permission_entry(&self) -> PermissionEntry {
        PermissionEntry::new(
            non_empty_string(&self.owner.user),
            non_empty_string(&self.owner.group),
            Some(self.mode),
        )
    }

    fn handle_permission_errors(&self, result: Result<(), PathsError>) -> Result<(), PathsError> {
        if self.warn_and_ignore_permission_errors {
            ignore_owner_or_mode_error(result)
        } else {
            result
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagedTemplateFile {
    active: ManagedFile,
    parent: Option<ManagedDir>,
    warn_and_ignore_permission_errors: bool,
}

impl ManagedTemplateFile {
    pub fn path(&self) -> &Path {
        self.active.path()
    }

    pub fn owner(&self) -> &Owner {
        self.active.owner()
    }

    pub fn with_owner(mut self, owner: Owner) -> Self {
        self.active = self.active.with_owner(owner);
        self
    }

    pub fn warn_and_ignore_permission_errors(mut self) -> Self {
        self.warn_and_ignore_permission_errors = true;
        self
    }

    pub async fn persist(self, content: impl AsRef<[u8]>) -> Result<(), PathsError> {
        let content = content.as_ref();
        let template = self.template_file();
        let disabled_path = append_path_suffix(self.active.path(), ".disabled");

        if let Some(parent) = &self.parent {
            parent.create_if_missing().await?;
        }

        let prior_config: Option<Vec<u8>> = tokio::fs::read(self.active.path()).await.ok();
        let prior_template: Option<Vec<u8>> = tokio::fs::read(template.path()).await.ok();
        let overridden = prior_config != prior_template;
        let disabled = tokio::fs::try_exists(&disabled_path).await.unwrap_or(false);

        if !overridden && !disabled {
            self.persist_file(&self.active, content).await?;
        }

        self.persist_file(&template, content).await
    }

    fn template_file(&self) -> ManagedFile {
        ManagedFile {
            path: append_path_suffix(self.active.path(), ".template"),
            owner: self.active.owner.clone(),
            mode: self.active.mode,
            warn_and_ignore_permission_errors: false,
        }
    }

    async fn persist_file(&self, file: &ManagedFile, content: &[u8]) -> Result<(), PathsError> {
        let result = file.replace_atomic(content).await;
        if self.warn_and_ignore_permission_errors {
            ignore_owner_or_mode_error(result)
        } else {
            result
        }
    }
}

fn append_path_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut path = path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then_some(value.to_string())
}

fn ignore_owner_or_mode_error(result: Result<(), PathsError>) -> Result<(), PathsError> {
    match result {
        Err(PathsError::FileError(
            err @ (FileError::MetaDataError { .. }
            | FileError::ChangeModeError { .. }
            | FileError::UserNotFound { .. }
            | FileError::GroupNotFound { .. }),
        )) => {
            warn!("{err}");
            Ok(())
        }
        result => result,
    }
}

fn validate_managed_path(path: &Path) -> Result<(), PathsError> {
    if path.is_absolute() {
        return Err(PathsError::InvalidManagedPath {
            path: path.to_path_buf(),
        });
    }

    for component in path.components() {
        match component {
            Component::CurDir | Component::Normal(_) => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PathsError::InvalidManagedPath {
                    path: path.to_path_buf(),
                })
            }
        }
    }

    Ok(())
}

pub fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileError;
    use nix::unistd::Uid;
    use std::os::unix::fs::PermissionsExt;
    use tedge_test_utils::fs::TempTedgeDir;
    use uzers::get_group_by_gid;

    fn current_owner() -> Owner {
        let user = whoami::username();
        let gid = nix::unistd::getgid().as_raw();
        let group = get_group_by_gid(gid)
            .expect("group must exist")
            .name()
            .to_string_lossy()
            .into_owned();
        Owner::user_group(user, group)
    }

    #[test]
    fn accepts_absolute_paths_under_the_root() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");

        let dir = root.dir("/etc/tedge/operations/c8y").unwrap();
        assert_eq!(dir.path(), Path::new("/etc/tedge/operations/c8y"));

        let file = root.file("/etc/tedge/system.toml").unwrap();
        assert_eq!(file.path(), Path::new("/etc/tedge/system.toml"));
    }

    #[test]
    fn rejects_absolute_paths_outside_the_root() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");
        let err = root.dir("/etc").unwrap_err();
        assert!(matches!(err, PathsError::PathOutsideRoot { .. }));
    }

    #[test]
    fn rejects_absolute_paths_that_escape_the_root() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");
        let err = root.file("/etc/tedge/../passwd").unwrap_err();
        assert!(matches!(err, PathsError::InvalidManagedPath { .. }));
    }

    #[test]
    fn rejects_parent_relative_paths() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");
        let err = root.file("../passwd").unwrap_err();
        assert!(matches!(err, PathsError::InvalidManagedPath { .. }));
    }

    #[tokio::test]
    async fn ensure_creates_missing_managed_directories_with_default_mode() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path();
        let owner = current_owner();
        let config_root =
            TedgePaths::from_root_with_defaults(root, owner.user.clone(), owner.group.clone());

        config_root
            .dir("operations/c8y")
            .unwrap()
            .ensure()
            .await
            .unwrap();

        assert!(root.join("operations").is_dir());
        assert!(root.join("operations/c8y").is_dir());
        assert_eq!(mode_bits(root.join("operations/c8y")).await, 0o755);
    }

    #[tokio::test]
    async fn group_writable_sets_group_write_bit() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path();
        let owner = current_owner();
        let config_root =
            TedgePaths::from_root_with_defaults(root, owner.user.clone(), owner.group.clone());

        config_root
            .dir("operations")
            .unwrap()
            .group_writable()
            .ensure()
            .await
            .unwrap();

        assert_eq!(mode_bits(root.join("operations")).await, 0o775);
    }

    #[tokio::test]
    async fn create_if_missing_creates_missing_parent_directories() {
        let ttd = TempTedgeDir::new();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(
            ttd.path(),
            owner.user.clone(),
            owner.group.clone(),
        );

        config_root
            .dir("operations/c8y")
            .unwrap()
            .create_if_missing()
            .await
            .unwrap();

        assert!(ttd.path().join("operations").is_dir());
        assert!(ttd.path().join("operations/c8y").is_dir());
    }

    #[tokio::test]
    async fn create_if_missing_leaves_existing_directory_mode_unchanged() {
        let ttd = TempTedgeDir::new();
        let existing = ttd.dir("operations");
        existing.set_mode(0o700);

        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(
            ttd.path(),
            owner.user.clone(),
            owner.group.clone(),
        );

        config_root
            .dir("operations")
            .unwrap()
            .create_if_missing()
            .await
            .unwrap();

        assert_eq!(mode_bits(existing.path()).await, 0o700);
    }

    #[tokio::test]
    async fn explicit_owner_override_replaces_default_owner() {
        let ttd = TempTedgeDir::new();
        let root = TedgePaths::from_root_with_defaults(ttd.path(), "tedge", "tedge");

        let file = root
            .file("mosquitto-conf/c8y-bridge.conf")
            .unwrap()
            .with_owner(Owner::user_group("mosquitto", "mosquitto"));

        assert_eq!(file.owner(), &Owner::user_group("mosquitto", "mosquitto"));
    }

    #[tokio::test]
    async fn file_create_if_missing_writes_content_on_first_call() {
        let ttd = TempTedgeDir::new();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .file("system.toml")
            .unwrap()
            .create_if_missing(b"# default config")
            .await
            .unwrap();

        let content = tokio::fs::read(ttd.path().join("system.toml"))
            .await
            .unwrap();
        assert_eq!(content, b"# default config");
    }

    #[tokio::test]
    async fn file_create_if_missing_does_not_overwrite_existing_content() {
        let ttd = TempTedgeDir::new();
        ttd.file("system.toml")
            .with_raw_content("# existing config");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .file("system.toml")
            .unwrap()
            .create_if_missing(b"# new content")
            .await
            .unwrap();

        let content = tokio::fs::read(ttd.path().join("system.toml"))
            .await
            .unwrap();
        assert_eq!(content, b"# existing config");
    }

    #[tokio::test]
    async fn file_create_if_missing_sets_mode() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(root, owner.user, owner.group);

        config_root
            .file("system.toml")
            .unwrap()
            .create_if_missing(b"")
            .await
            .unwrap();

        assert_eq!(mode_bits(root.join("system.toml")).await, 0o644);
    }

    #[tokio::test]
    async fn file_create_if_missing_fails_when_parent_missing() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path().join("root-not-created");
        // root is intentionally not created
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(&root, owner.user, owner.group);

        let err = config_root
            .file("system.toml")
            .unwrap()
            .create_if_missing(b"")
            .await
            .unwrap_err();

        assert!(matches!(err, PathsError::IoError(_)));
    }

    #[tokio::test]
    async fn file_create_if_missing_with_wrong_user() {
        let ttd = TempTedgeDir::new();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        let err = config_root
            .file("system.toml")
            .unwrap()
            .with_owner(Owner::user_group("nonexistent_user", "root"))
            .create_if_missing(b"")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[tokio::test]
    async fn ensure_with_wrong_user() {
        let ttd = TempTedgeDir::new();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        let err = config_root
            .dir("operations")
            .unwrap()
            .with_owner(Owner::user_group("nonexistent_user", "root"))
            .ensure()
            .await
            .unwrap_err();

        assert!(err.to_string().contains("User not found"));
    }

    #[tokio::test]
    async fn ensure_reports_the_failing_ancestor_path() {
        if Uid::current().is_root() {
            return;
        }

        let ttd = TempTedgeDir::new();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), "root", "root");
        let failing_ancestor = ttd.path().join("operations");
        let requested = ttd.path().join("operations").join("c8y");

        let err = config_root
            .dir("operations/c8y")
            .unwrap()
            .ensure()
            .await
            .unwrap_err();

        let err = err.to_string();
        assert!(err.contains(&failing_ancestor.display().to_string()));
        assert!(!err.contains(&requested.display().to_string()));
    }

    #[tokio::test]
    async fn ensure_does_not_create_directories_above_the_root() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path().join("missing-parent").join("managed-root");
        let config_root = TedgePaths::from_root_with_defaults(&root, "", "");

        let err = config_root
            .dir("operations/c8y")
            .unwrap()
            .ensure()
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            PathsError::FileError(FileError::DirectoryCreateFailed { .. })
        ));
        assert!(!root.exists());
        assert!(!ttd.path().join("missing-parent").exists());
    }

    #[tokio::test]
    async fn replace_atomic_preserves_symlink_following_behavior() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path();
        let bridge_conf = ttd
            .dir("mosquitto-conf")
            .file("c8y-bridge.conf")
            .with_raw_content("before");
        let link = root.join("bridge-link.conf");
        std::os::unix::fs::symlink(bridge_conf.path(), &link).unwrap();

        let owner = current_owner();
        let config_root =
            TedgePaths::from_root_with_defaults(root, owner.user.clone(), owner.group.clone());

        config_root
            .file("bridge-link.conf")
            .unwrap()
            .replace_atomic("after")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(&bridge_conf.path())
                .await
                .unwrap(),
            "after"
        );
    }

    #[tokio::test]
    async fn template_persistence_creates_active_and_template_files() {
        let ttd = TempTedgeDir::new();
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("test content")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml"))
                .await
                .unwrap(),
            "test content"
        );
        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml.template"))
                .await
                .unwrap(),
            "test content"
        );
    }

    #[tokio::test]
    async fn template_persistence_updates_both_files_when_active_matches_template() {
        let ttd = TempTedgeDir::new();
        ttd.dir("bridge");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("old content")
            .await
            .unwrap();
        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("new content")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml"))
                .await
                .unwrap(),
            "new content"
        );
        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml.template"))
                .await
                .unwrap(),
            "new content"
        );
    }

    #[tokio::test]
    async fn template_persistence_preserves_overridden_active_file() {
        let ttd = TempTedgeDir::new();
        ttd.dir("bridge");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("old content")
            .await
            .unwrap();
        tokio::fs::write(ttd.path().join("bridge/rules.toml"), "custom content")
            .await
            .unwrap();
        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("new content")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml"))
                .await
                .unwrap(),
            "custom content"
        );
        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml.template"))
                .await
                .unwrap(),
            "new content"
        );
    }

    #[tokio::test]
    async fn template_persistence_preserves_disabled_active_file() {
        let ttd = TempTedgeDir::new();
        ttd.dir("bridge");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("old content")
            .await
            .unwrap();
        tokio::fs::write(ttd.path().join("bridge/rules.toml.disabled"), "")
            .await
            .unwrap();
        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .persist("new content")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml"))
                .await
                .unwrap(),
            "old content"
        );
        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml.template"))
                .await
                .unwrap(),
            "new content"
        );
    }

    #[tokio::test]
    async fn template_persistence_can_warn_and_ignore_owner_or_mode_errors() {
        if Uid::current().is_root() {
            return;
        }

        let ttd = TempTedgeDir::new();
        ttd.dir("bridge");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        let err = config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .with_owner(Owner::user_group("root", "root"))
            .persist("test content")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Failed to change owner"));

        let ttd = TempTedgeDir::new();
        ttd.dir("bridge");
        let owner = current_owner();
        let config_root = TedgePaths::from_root_with_defaults(ttd.path(), owner.user, owner.group);

        config_root
            .template_file("bridge/rules.toml")
            .unwrap()
            .with_owner(Owner::user_group("root", "root"))
            .warn_and_ignore_permission_errors()
            .persist("test content")
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml"))
                .await
                .unwrap(),
            "test content"
        );
        assert_eq!(
            tokio::fs::read_to_string(ttd.path().join("bridge/rules.toml.template"))
                .await
                .unwrap(),
            "test content"
        );
    }

    async fn mode_bits(path: impl AsRef<std::path::Path>) -> u32 {
        tokio::fs::metadata(path.as_ref())
            .await
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }
}
