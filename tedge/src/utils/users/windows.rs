#[derive(thiserror::Error, Debug)]
pub enum UserSwitchError {
    #[error("Tried to become user, but it did not exist: {name}")]
    UnknownUser {
        name: String
    },

    #[error("Tried to become group, but it did not exist: {name}")]
    UnknownGroup {
        name: String
    },

    #[error(transparent)]
    NotAuthorized(#[from] std::io::Error)
}

pub struct UserGuard {}

pub fn become_user(_username: &str) -> Result<UserGuard, UserSwitchError> {
    Ok(UserGuard{})
}
