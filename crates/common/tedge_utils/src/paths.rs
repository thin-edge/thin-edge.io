use std::ffi::OsString;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

use crate::file::permissions;
use crate::file::PermissionEntry;
use crate::fs::atomically_write_file_async;

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

    pub fn dir(&self, relative_path: impl AsRef<Path>) -> Result<ManagedDir, PathsError> {
        Ok(ManagedDir {
            root: self.root.clone(),
            path: self.resolve(relative_path)?,
            owner: self.default_owner.clone(),
            mode: 0o755,
        })
    }

    pub fn file(&self, relative_path: impl AsRef<Path>) -> Result<ManagedFile, PathsError> {
        Ok(ManagedFile {
            path: self.resolve(relative_path)?,
            owner: self.default_owner.clone(),
            mode: 0o644,
        })
    }

    fn resolve(&self, relative_path: impl AsRef<Path>) -> Result<PathBuf, PathsError> {
        let relative_path = relative_path.as_ref();
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

    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    pub async fn ensure(&self) -> Result<(), PathsError> {
        let permissions = self.permission_entry().force_dir_ownership();
        permissions
            .create_directory_with_root(self.path(), &self.root)
            .await?;
        Ok(())
    }

    pub async fn create_if_missing(&self) -> Result<(), PathsError> {
        let permissions = self.permission_entry();
        permissions.create_directory(self.path()).await?;
        Ok(())
    }

    fn permission_entry(&self) -> PermissionEntry {
        permissions(&self.owner.user, &self.owner.group, self.mode)
    }
}

#[derive(Debug, Clone)]
pub struct ManagedFile {
    path: PathBuf,
    owner: Owner,
    mode: u32,
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

    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    pub async fn replace_atomic(&self, content: impl AsRef<[u8]>) -> Result<(), PathsError> {
        atomically_write_file_async(&self.path, content.as_ref()).await?;
        self.permission_entry().apply(&self.path).await?;
        Ok(())
    }

    pub async fn create_if_missing(&self, content: impl AsRef<[u8]>) -> Result<(), PathsError> {
        let mut options = tokio::fs::OpenOptions::new();
        match options.create_new(true).write(true).open(&self.path).await {
            Ok(mut file) => {
                file.write_all(content.as_ref()).await?;
                file.flush().await?;
                file.sync_all().await?;
                self.permission_entry().apply(&self.path).await?;
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    fn permission_entry(&self) -> PermissionEntry {
        permissions(&self.owner.user, &self.owner.group, self.mode)
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
    fn rejects_absolute_paths() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");
        let err = root.dir("/etc").unwrap_err();
        assert!(matches!(err, PathsError::InvalidManagedPath { .. }));
    }

    #[test]
    fn rejects_parent_relative_paths() {
        let root = TedgePaths::from_root_with_defaults("/etc/tedge", "tedge", "tedge");
        let err = root.file("../passwd").unwrap_err();
        assert!(matches!(err, PathsError::InvalidManagedPath { .. }));
    }

    #[tokio::test]
    async fn ensure_creates_missing_managed_directories() {
        let ttd = TempTedgeDir::new();
        let root = ttd.path();
        let owner = current_owner();
        let config_root =
            TedgePaths::from_root_with_defaults(root, owner.user.clone(), owner.group.clone());

        config_root
            .dir("operations/c8y")
            .unwrap()
            .with_mode(0o755)
            .ensure()
            .await
            .unwrap();

        assert!(root.join("operations").is_dir());
        assert!(root.join("operations/c8y").is_dir());
        assert_eq!(mode_bits(root.join("operations/c8y")).await, 0o755);
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
            .with_mode(0o755)
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
            .with_mode(0o640)
            .create_if_missing(b"")
            .await
            .unwrap();

        assert_eq!(mode_bits(root.join("system.toml")).await, 0o640);
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
            .with_mode(0o755)
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

    async fn mode_bits(path: impl AsRef<std::path::Path>) -> u32 {
        tokio::fs::metadata(path.as_ref())
            .await
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }
}
