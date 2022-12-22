use std::io;
use std::path::Path;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct Flockfile {
    pub path: PathBuf,
}

impl Flockfile {
    pub fn new_lock(lock_name: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        let path = Path::new("/no/lock/on/windows").join(lock_name);
        Ok(Flockfile { path: path })
    }

    pub fn unlock(self) -> Result<(), io::Error> {
        Ok(())
    }
}

impl AsRef<Path> for Flockfile {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}
