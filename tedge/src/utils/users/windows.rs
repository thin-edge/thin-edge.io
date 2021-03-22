pub struct UserGuard {}

pub fn become_user(_username: &str) -> Result<UserGuard, super::UserSwitchError> {
    Ok(UserGuard {})
}
