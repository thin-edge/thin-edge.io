use chrono::offset::FixedOffset;
use chrono::DateTime;

/// The `MeasurementVisitor` trait represents the capability to visit a series of measurements, possibly grouped.
///
/// Here is an implementation of the `MeasurementVisitor` trait that prints the measurements:
///
/// ```
/// # use thin_edge_json::measurement::*;
/// # use chrono::*;
/// struct MeasurementPrinter {
///     group: Option<String>,
/// }
///
/// #[derive(thiserror::Error, Debug)]
/// pub enum MeasurementError {
///     #[error("Unexpected time stamp within a group")]
///     UnexpectedTimestamp,
///
///     #[error("Unexpected end of group")]
///     UnexpectedEndOfGroup,
///
///     #[error("Unexpected start of group")]
///     UnexpectedStartOfGroup,
/// }
///
/// impl MeasurementVisitor for MeasurementPrinter {
///     type Error = MeasurementError;
///
///     fn visit_timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             Ok(println!("time = {}", timestamp.to_rfc2822()))
///         } else {
///             Err(MeasurementError::UnexpectedTimestamp)
///         }
///     }
///
///     fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
///         if let Some(group_name) = self.group.as_ref() {
///             Ok(println!("{}.{} = {}", group_name, name, value))
///         } else {
///             Ok(println!("{} = {}", name, value))
///         }
///     }
///
///     fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             self.group = Some(group.to_owned());
///             Ok(())
///         } else {
///             Err(MeasurementError::UnexpectedStartOfGroup)
///         }
///     }
///
///     fn visit_end_group(&mut self) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             Err(MeasurementError::UnexpectedEndOfGroup)
///         } else {
///             self.group = None;
///             Ok(())
///         }
///     }
/// }
/// ```
pub trait MeasurementVisitor {
    /// Error type specific to this visitor.
    type Error: std::error::Error + std::fmt::Debug;

    /// Set the timestamp shared by all the measurements of this series.
    fn visit_timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;

    /// Add a new measurement, attached to the current group if any.
    fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;

    /// Start to gather measurements for a group.
    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error>;

    /// End to gather measurements for the current group.
    fn visit_end_group(&mut self) -> Result<(), Self::Error>;
}
