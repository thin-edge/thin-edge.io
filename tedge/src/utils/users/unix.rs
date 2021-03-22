pub struct UserGuard {
    inner: Option<users::switch::SwitchUserGuard>,
}

impl UserGuard {
    fn current_user() -> UserGuard {
        UserGuard { inner: None }
    }

    fn switched_user(guard: users::switch::SwitchUserGuard) -> UserGuard {
        UserGuard {
            inner: Some(guard)
        }
    }
}

pub fn become_user(username: &str) -> Result<UserGuard, super::UserSwitchError> {
    println!("Becoming user: {}", username);
    println!("Current user: {}", users::get_current_uid());
    println!("Effective user: {}", users::get_effective_uid());

    if users::get_current_uid() == 0 {
        // root has uid 0
        let user =
            users::get_user_by_name(username).ok_or_else(|| super::UserSwitchError::UnknownUser {
                name: username.to_owned(),
            })?;

        let group =
            users::get_group_by_name(username).ok_or_else(|| super::UserSwitchError::UnknownGroup {
                name: username.to_owned(),
            })?;

        let uid = user.uid();
        let gid = group.gid();

        let guard = users::switch::switch_user_group(uid, gid)?;
        println!("Successfully switched user");
        Ok(UserGuard::switched_user(guard))
    } else {
        Ok(UserGuard::current_user())
    }
}
