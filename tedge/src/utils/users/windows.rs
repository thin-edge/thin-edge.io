use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone)]
pub struct UserManager {
    _force_not_send: PhantomData<Rc<()>>,
}

pub struct UserGuard {
    _force_not_send: PhantomData<Rc<()>>,
}

impl UserManager {
    pub fn new() -> UserManager {
        UserManager {
            _force_not_send: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn running_as_root() -> bool {
        false
    }

    pub fn become_user(&self, _username: &str) -> Result<UserGuard, super::UserSwitchError> {
        Ok(UserGuard {
            _force_not_send: PhantomData,
        })
    }
}
