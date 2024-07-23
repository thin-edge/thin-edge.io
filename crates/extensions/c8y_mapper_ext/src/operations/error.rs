use std::error::Error;
use std::fmt::Display;

/// An error type for operation handlers.
///
/// This error type wraps an [`anyhow::Error`] and provides a display implementation that assumes
/// that source errors print their own causes after `:` character. This is in order to ensure that
/// failures are printed properly in Cumulocity web interface.
#[derive(Debug)]
pub(crate) struct OperationError(anyhow::Error);

impl Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)?;

        if let Some(source) = self.0.source() {
            write!(f, ": {}", source)?;
        }

        Ok(())
    }
}

impl Error for OperationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl From<anyhow::Error> for OperationError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn separates_error_levels_on_a_single_line() {
        let example_io_err: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Example io error",
        ));

        let operation_error: OperationError = example_io_err
            .context("Could not perform io operation")
            .unwrap_err()
            .into();

        assert_eq!(
            &operation_error.to_string(),
            "Could not perform io operation: Example io error"
        );
    }
}
