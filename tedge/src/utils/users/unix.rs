use std::rc::Rc;
use std::sync::Mutex;

/// The `UserManager` allows the process to switch from one unix user to another.
///
/// * If the process is running as root, then the method `UserManager::become_user()`
///   is effective and the process can switch back and force to different users.
/// * If the process is not running as root, then the method `UserManager::become_user()`
///   has no effects. Note that no error is raised.
///
///   The rational is that a `tedge` command running as root (i.e. using `sudo tedge`)
///   has a fine grain control over the different operations and files,
///   while the unprivileged `tedge` command creates all the files using the former user.
///
#[derive(Clone)]
pub struct UserManager {
    // This implementation can never thread-safe because the current user is a global concept for the process.
    // If one thread changes the user, it affects another thread that might have wanted a different user.
    // So, let's use Rc rather than Arc to force !Send.
    inner: Rc<Mutex<InnerUserManager>>,
}

struct InnerUserManager {
    users: Vec<String>,
    guard: Option<users::switch::SwitchUserGuard>,
}

impl UserManager {
    /// Create a `UserManager`.
    ///
    /// This function MUST be called only once.
    /// But be warned, the compiler will not prevent you to call it twice.
    /// If you do so, one thread might be switched by another thread to some un-expected user.
    ///
    /// This struct is not `Send` amd cannot be shared between thread.
    pub fn new() -> UserManager {
        UserManager {
            inner: Rc::new(Mutex::new(InnerUserManager {
                users: vec![],
                guard: None,
            })),
        }
    }

    /// Check if the process has been launched using `sudo` or not.
    ///
    /// ```
    ///     let path = if UserManager::running_as_root() {
    ///         "/etc/mosquitto/mosquitto.conf"
    ///      } else {
    ///          home_dir(".tedge/mosquitto.conf")
    ///      };
    /// ```
    pub fn running_as_root() -> bool {
        users::get_current_uid() == 0
    }

    /// Switch the effective user of the running process.
    ///
    /// This method returns a guard. As long as the guard is owner by the caller,
    /// the process is running under the requested user. When the guard is dropped,
    /// then the process switches back to the former user. These calls can be stacked.
    ///
    /// ```
    /// fn main()  {
    ///     // Running as root
    ///
    ///     let user_manager = UserManager::new();
    ///     let _user_guard_1 = user_manager.become_user("user_1")?;
    ///
    ///     // Running as user1
    ///
    ///     {
    ///          let _user_guard_2 = user_manager.become_user("user_2")?;
    ///
    ///          // Running as user2
    ///     }
    ///
    ///     // Running as user1
    /// }
    /// ```
    ///
    /// If the process is not running as root, the user is unchanged,
    /// no error is raised and a dummy guard is returned.
    /// In other words, a process running as root can have a fine control of the different permission modes,
    /// while the same program running under a non-privileged user will perform the same operations
    /// but all using the same permission mode.
    /// For that to work, appropriate user-accessible resources will have to be used.
    ///
    /// For example, running as root, the process can read the configuration file as the tedge user,
    /// then create a private key as mosquitto and restart mosquitto using systemd as root.
    /// The same process, running as the `pi` user, operates as `pi` for all the operations.
    /// Hence, the configuration files and the private key are to be owned by `pi`. Similarly,
    /// the mosquitto process has to be run as `pi`.
    ///
    ///  The function returns a `UserSwitchError` if the given user is unknown.
    ///
    pub fn become_user(&self, username: &str) -> Result<UserGuard, super::UserSwitchError> {
        if UserManager::running_as_root() {
            self.inner.lock().unwrap().become_user(username)?;
        }

        Ok(UserGuard {
            user_manager: self.clone(),
        })
    }

    fn drop_guard(&self) {
        let mut lock_guard = self.inner.lock().unwrap();
        lock_guard.drop_guard()
    }
}

impl InnerUserManager {
    fn become_user(&mut self, username: &str) -> Result<(), super::UserSwitchError> {
        self.guard.take();

        match InnerUserManager::inner_become_user(username) {
            Ok(guard) => {
                self.guard = Some(guard);
                self.users.push(username.to_owned());
                Ok(())
            }
            Err(err) => {
                self.inner_restore_previous_user();
                Err(err)
            }
        }
    }

    fn drop_guard(&mut self) {
        self.guard.take();

        if let None = self.users.pop() {
            return;
        }

        self.inner_restore_previous_user();
    }

    fn inner_restore_previous_user(&mut self) {
        if let Some(username) = self.users.last() {
            let guard = InnerUserManager::inner_become_user(username).expect(&format!(
                r#"Fail to switch back to the former user: {}.
                Has this user been removed from the system?
                Aborting to avoid any security issue."#,
                username
            ));
            self.guard = Some(guard);
        }
    }

    fn inner_become_user(
        username: &str,
    ) -> Result<users::switch::SwitchUserGuard, super::UserSwitchError> {
        let user = users::get_user_by_name(username).ok_or_else(|| {
            super::UserSwitchError::UnknownUser {
                name: username.to_owned(),
            }
        })?;

        let group = users::get_group_by_name(username).ok_or_else(|| {
            super::UserSwitchError::UnknownGroup {
                name: username.to_owned(),
            }
        })?;

        let uid = user.uid();
        let gid = group.gid();

        Ok(users::switch::switch_user_group(uid, gid)?)
    }
}

/// Materialize the fact the process is running under a user different from the former one.
/// On drop the process switches back to the former user.
///
/// Such a guard implements the RAII pattern and provides no methods beyond `drop`.
///
/// fn create_certificate(&self, user_manager: &UserManager) -> Result<(), CertError> {
///     let _user_guard = user_manager.become_user("mosquitto")?;
///     // As long as the _user_guard is owned, the process run as mosquitto.
///
///     // Create the certificate on behalf of mosquitto.
///
///     Ok(())
///     // Here, the _user_guard is dropped and the process switches back to the former user.
/// }
pub struct UserGuard {
    user_manager: UserManager,
}

impl Drop for UserGuard {
    fn drop(&mut self) {
        self.user_manager.drop_guard();
    }
}
