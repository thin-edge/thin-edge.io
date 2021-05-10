use chrono::offset::FixedOffset;
use chrono::DateTime;

/// The `FlatMeasurementVisitor` trait represents the capability to collect/visit a series of measurements.
///
/// An implementation of this trait is a consumer to which a producer forwards the measurements one after the other.
///
/// ```
/// use thin_edge_json::measurement::*;
/// use thin_edge_json::group::MeasurementGrouper;
/// # fn main() -> Result<(), anyhow::Error> {
///
/// // A producer first needs a consumer to forward the source measurements.
/// let mut consumer = MeasurementGrouper::new();
///
/// // Then the producer can forward the different measurements,
/// // possibly attaching each measurement to a group
/// consumer.measurement(Some("group-name"), "measurement-name", 42.0)?;
/// consumer.measurement(None, "measurement-unrelated-to-any-group", 42.0)?;
///
/// // The measurements can be sourced in an order that is independent of the groups.
/// consumer.measurement(Some("g1"), "x", 1.0)?;
/// consumer.measurement(Some("g2"), "a", 2.0)?;
/// consumer.measurement(Some("g1"), "y", 3.0)?;
/// consumer.measurement(None, "k", 1.0)?;
/// consumer.measurement(Some("g2"), "b", 4.0)?;
///
/// // A timestamp can be assigned to the whole measurement series
/// consumer.timestamp(&current_timestamp())?;
///
/// // Note that the timestamp or a measurement can be pushed several times.
/// // __However__, the behavior depends on the actual consumer.
/// // Here, the consumer is a `MeasurementGrouper` that simply always peeks the latest value.
/// consumer.timestamp(&current_timestamp())?;         // update the timestamp
/// consumer.measurement(Some("g1"), "x", 2.0)?;       // update g1.x
/// consumer.measurement(None, "k", 2.0)?;             // update k
///
/// # Ok(()) }
/// ```
///
/// Here is an implementation that simply prints the series of measurements as they are produced.
///
/// ```
/// # use thin_edge_json::measurement::*;
/// # use chrono::*;
/// struct MeasurementPrinter {}
///
/// impl FlatMeasurementVisitor for MeasurementPrinter {
///    type Error = ();
///
///     fn timestamp(&mut self, value: &DateTime<FixedOffset>) -> Result<(), Self::Error> {
///         Ok(println!("time = {}", value.to_rfc2822()))
///     }
///
///     fn measurement(
///         &mut self,
///         group: Option<&str>,
///         name: &str, value: f64
///     ) -> Result<(), Self::Error> {
///         if let Some(group_name) = group {
///             Ok(println!("{}.{} = {}", group_name, name, value))
///         } else {
///             Ok(println!("{} = {}", name, value))
///
///         }
///     }
/// }
/// ```
pub trait FlatMeasurementVisitor {
    /// Error type specific to this way of collecting measurements
    type Error;

    /// Set the timestamp shared by all the measurements of this series
    fn timestamp(&mut self, value: &DateTime<FixedOffset>) -> Result<(), Self::Error>;

    /// Add a new measurement, possibly attached to a group
    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error>;
}

/// The `GroupedMeasurementVisitor` trait represents the capability
/// to collect/visit a series of measurements that have *already* been arranged in groups.
///
/// This trait represents the interface between a source and a consumer of measurements
/// with the assumptions that measurements of two different groups
/// will never be produced in an interleaved order.
/// Such a garanty, allows the implementation of streaming consumers.
///
/// Here is an implementation of the `GroupedMeasurementVisitor` trait that prints the measurememts.
///
/// ```
/// # use thin_edge_json::measurement::*;
/// # use chrono::*;
/// struct GroupedMeasurementPrinter {
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
/// impl GroupedMeasurementVisitor for GroupedMeasurementPrinter {
///     type Error = MeasurementError;
///
///     fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             Ok(println!("time = {}", value.to_rfc2822()))
///         } else {
///             Err(MeasurementError::UnexpectedTimestamp)
///         }
///     }
///
///     fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
///         if let Some(group_name) = self.group.as_ref() {
///             Ok(println!("{}.{} = {}", group_name, name, value))
///         } else {
///             Ok(println!("{} = {}", name, value))
///         }
///     }
///
///     fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             self.group = Some(group.to_owned());
///             Ok(())
///         } else {
///             Err(MeasurementError::UnexpectedStartOfGroup)
///         }
///     }
///
///     fn end_group(&mut self) -> Result<(), Self::Error> {
///         if self.group.is_none() {
///             Err(MeasurementError::UnexpectedEndOfGroup)
///         } else {
///             self.group = None;
///             Ok(())
///         }
///     }
/// }
/// ```
pub trait GroupedMeasurementVisitor {
    /// Error type specific to this way of collecting measurements
    type Error: std::error::Error + std::fmt::Debug;

    /// Set the timestamp shared by all the measurements of this serie
    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;

    /// Start to gather measurements for a group
    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;

    /// Definitely end to gather measurements for the current group
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;

    /// Add a new measurement, attached to the current group if any
    fn end_group(&mut self) -> Result<(), Self::Error>;
}

/// Return the current timestamp using the local time zone
pub fn current_timestamp() -> DateTime<FixedOffset> {
    let local_time_now: DateTime<chrono::Local> = chrono::Local::now();
    local_time_now.with_timezone(local_time_now.offset())
}
