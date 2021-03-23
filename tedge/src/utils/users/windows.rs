#[derive(Clone)]
pub struct UserManager {}

pub struct UserGuard {}

impl UserManager {
    pub fn new() -> UserManager {
        UserManager {}
    }

    pub fn running_as_root() -> bool {
        false
    }

    pub fn become_user(&self, _username: &str) -> Result<UserGuard, super::UserSwitchError> {
        Ok(UserGuard {})
    }
}
