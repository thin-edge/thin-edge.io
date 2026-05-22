use uzers::get_current_gid;
use uzers::get_current_uid;
use uzers::get_group_by_gid;
use uzers::get_user_by_uid;

/// Returns the username of the current process's effective user.
pub fn current_username() -> String {
    get_user_by_uid(get_current_uid())
        .expect("current user must exist")
        .name()
        .to_string_lossy()
        .into_owned()
}

/// Returns the group name of the current process's primary group.
pub fn current_groupname() -> String {
    get_group_by_gid(get_current_gid())
        .expect("current group must exist")
        .name()
        .to_string_lossy()
        .into_owned()
}
