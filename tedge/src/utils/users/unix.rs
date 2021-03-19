
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

pub struct UserGuard {
    _guard: Option<users::switch::SwitchUserGuard>,
}

impl UserGuard {
    fn current_user() -> UserGuard {
        UserGuard { _guard: None }
    }
}

impl From<users::switch::SwitchUserGuard> for UserGuard {
    fn from(guard: users::switch::SwitchUserGuard) -> Self {
        UserGuard { _guard: Some(guard) }
    }
}

pub fn become_user(username: &str) -> Result<UserGuard, UserSwitchError> {
    if users::get_current_uid() == 0 { // root has uid 0
        let user = users::get_user_by_name(username)
            .ok_or_else(|| UserSwitchError::UnknownUser { name: username.to_owned() })?;

        let group = users::get_group_by_name(username)
            .ok_or_else(|| UserSwitchError::UnknownGroup { name: username.to_owned() })?;

        let uid = user.uid();
        let gid = group.gid();

        Ok(users::switch::switch_user_group(uid, gid)?.into())
    } else {
        Ok(UserGuard::current_user())
    }
}

