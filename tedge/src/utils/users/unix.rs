use std::rc::Rc;
use std::sync::Mutex;

// This implementation can never thread-safe because the current user is a global concept for the process.
// If one thread changes the user, it affects another thread that might have wanted a different user.
// So, let's use Rc rather than Arc to force !Send.
#[derive(Clone)]
pub struct UserManager {
    inner: Rc<Mutex<InnerUserManager>>,
}

struct InnerUserManager {
    users: Vec<String>,
    guard: Option<users::switch::SwitchUserGuard>,
}

impl UserManager {
    pub fn new() -> UserManager {
        UserManager {
            inner: Rc::new(Mutex::new(InnerUserManager {
                users: vec![],
                guard: None,
            })),
        }
    }

    pub fn running_as_root() -> bool {
        users::get_current_uid() == 0
    }

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

pub struct UserGuard {
    user_manager: UserManager,
}

impl Drop for UserGuard {
    fn drop(&mut self) {
        self.user_manager.drop_guard();
    }
}
