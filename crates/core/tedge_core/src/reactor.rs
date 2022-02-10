use crate::TedgeApplication;
use crate::errors::Result;

/// Helper type for running a TedgeApplication
///
/// This type is only introduced for more seperation-of-concerns in the codebase
/// `Reactor::run()` is simply `TedgeApplication::run()`.
pub struct Reactor(pub TedgeApplication);

impl Reactor {
    pub async fn run(self) -> Result<()> {
        Ok(())
    }
}

