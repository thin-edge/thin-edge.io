use syn::Error;

/// Represents an `Option<syn::Error>`.
#[derive(Default)]
pub struct OptionalError(Option<Error>);

impl OptionalError {
    /// Create a new [`OptionalError`] with the given [error](Error).
    pub fn new(error: Error) -> Self {
        Self(Some(error))
    }

    /// Returns a reference to the contained [error](Error), if any.
    pub fn error(&self) -> Option<&Error> {
        self.0.as_ref()
    }

    /// Returns a mutable reference to the contained [error](Error), if any.
    pub fn error_mut(&mut self) -> Option<&mut Error> {
        self.0.as_mut()
    }

    /// Removes the contained [error](Error) and returns it, if any.
    pub fn take(&mut self) -> Option<Error> {
        self.0.take()
    }

    /// Replaces the contained [error](Error) with the given one.
    ///
    /// Returns the previous error, if any.
    pub fn replace(&mut self, error: Error) -> Option<Error> {
        self.0.replace(error)
    }

    /// Combine the given [error](Error) with the existing one,
    /// initializing it if none currently exists.
    pub fn combine(&mut self, error: Error) {
        match self.0 {
            None => self.0 = Some(error),
            Some(ref mut prev) => prev.combine(error),
        }
    }

    /// Combine the given [error](Error) with the existing one,
    /// initializing it if none currently exists.
    pub fn combine_owned(mut self, error: Error) -> Self {
        match self.0 {
            None => self.0 = Some(error),
            Some(ref mut prev) => prev.combine(error),
        };
        self
    }

    /// Returns a [`Result`] with the contained [error](Error), if any.
    ///
    /// This can be used for quick and easy early returns.
    pub fn try_throw(self) -> Result<(), Error> {
        match self.0 {
            None => Ok(()),
            Some(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;

    #[test]
    fn should_combine() {
        let mut collector = OptionalError::new(Error::new(Span::call_site(), "First Error"));
        collector.combine(Error::new(Span::call_site(), "Second Error"));

        let expected = r#":: core :: compile_error ! { "First Error" } :: core :: compile_error ! { "Second Error" }"#;
        let received = collector
            .try_throw()
            .expect_err("expected error")
            .to_compile_error()
            .to_string();
        assert_eq!(expected, received);
    }

    #[test]
    fn should_replace() {
        let mut collector = OptionalError::new(Error::new(Span::call_site(), "First Error"));
        let existing = collector.replace(Error::new(Span::call_site(), "Second Error"));

        let expected = r#":: core :: compile_error ! { "First Error" }"#;
        let received = existing
            .expect("expected error")
            .to_compile_error()
            .to_string();
        assert_eq!(expected, received);

        let expected = r#":: core :: compile_error ! { "Second Error" }"#;
        let received = collector
            .try_throw()
            .expect_err("expected error")
            .to_compile_error()
            .to_string();
        assert_eq!(expected, received);
    }

    #[test]
    fn should_take() {
        let mut collector = OptionalError::new(Error::new(Span::call_site(), "First Error"));
        let existing = collector.take();

        let expected = r#":: core :: compile_error ! { "First Error" }"#;
        let received = existing
            .expect("expected error")
            .to_compile_error()
            .to_string();
        assert_eq!(expected, received);
        assert!(collector.try_throw().is_ok());
    }
}
