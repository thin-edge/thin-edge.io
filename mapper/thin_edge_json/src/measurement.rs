use chrono::offset::FixedOffset;
use chrono::DateTime;
/// The FlatMeasurementVisitor trait is used to group the measurements from descrete measurements.
/// The descrete measurement that contains key, value or group, key, value.
/// All these are grouped as one ThinEdge Json message
/// For example the below example shows how the MeasurementGrouper implements FlatMeasurementVisitor to group
/// the messages.
/// ```
/// use thin_edge_json::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};
/// use thin_edge_json::serialize::ThinEdgeJsonSerializationError;
/// use thin_edge_json::group::MeasurementGrouperError;
/// use chrono::offset::FixedOffset;
/// use chrono::DateTime;
/// use chrono::TimeZone;
/// use std::collections::HashMap;
/// #[derive(Debug)]
/// pub struct MeasurementGrouper {
///     pub timestamp: DateTime<FixedOffset>,
///     pub values: HashMap<String, Measurement>,
/// }
///
/// #[derive(Debug)]
/// pub enum Measurement {
///     Single(f64),
///     Multi(HashMap<String, f64>),
/// }
///
/// impl FlatMeasurementVisitor for MeasurementGrouper {
///     type Error = MeasurementGrouperError;
///     fn timestamp(&mut self, timestamp: &DateTime<FixedOffset>) -> Result<(), Self::Error> {Ok(())}
///     fn measurement(&mut self,group: Option<&str>,name: &str,value: f64,) -> Result<(), Self::Error> {Ok(())}
/// }
///
/// fn test_timestamp() -> DateTime<FixedOffset> {
///    FixedOffset::east(5 * 3600)
///    .ymd(2021, 04, 08)
///    .and_hms(0, 0, 0)
///  }
/// impl MeasurementGrouper {
///   pub fn new(timestamp: DateTime<FixedOffset>) -> Self {
///     Self {
///        timestamp,
///        values: HashMap::new(),
///     }
///   }
///  }
///
/// fn tej_build_serialize() -> Result<(), MeasurementGrouperError> {
///     let mut grp_msg = MeasurementGrouper::new(test_timestamp());
///     grp_msg.timestamp(&test_timestamp())?;
//     !measurement that does not belong to any group
///     grp_msg.measurement(None, "temperature", 25.0)?;
//     !measurement that belongs to a group
///     grp_msg.measurement(Some("location"), "alti", 2100.4)?;
///     grp_msg.measurement(Some("location"), "longi", 2100.4)?;
///     grp_msg.measurement(Some("location"), "lati", 2100.4)?;
///     grp_msg.measurement(Some("location"), "alti", 2100.5)?;
///     Ok(())
/// }
/// ```
/// output message looks like below
///
/// {
///   "time": "2021-04-08T00:00:00+05:00",
///   "temperature": 25.0,
///    "location":
///        {
///            "lati": 2100.4,
///            "alti": 2100.5,
///            "longi": 2100.4,
///        },
/// }
///

pub trait FlatMeasurementVisitor {
    type Error;

    fn timestamp(&mut self, value: &DateTime<FixedOffset>) -> Result<(), Self::Error>;
    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error>;
}

/// The GroupedMeasurementVisitor trait manipulate the grouped messages.
/// For example ThinEdgeJsonSerializer implements the GroupedMeasurementVisitor
/// trait, to serialize the grouped Thinedge Json message.
/// ```
/// use thin_edge_json::measurement::{FlatMeasurementVisitor, GroupedMeasurementVisitor};
/// use thin_edge_json::serialize::ThinEdgeJsonSerializationError;
/// use chrono::offset::FixedOffset;
/// use chrono::DateTime;
/// use std::io::Write;
///
/// pub struct ThinEdgeJsonSerializer {
///        buffer: Vec<u8>,
///        is_within_group: bool,
///        needs_separator: bool,
///  }
///
/// impl GroupedMeasurementVisitor for ThinEdgeJsonSerializer {
///    type Error = ThinEdgeJsonSerializationError;
///    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>{Ok(())}
///    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>{Ok(())}
///    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>{Ok(())}
///    fn end_group(&mut self) -> Result<(), Self::Error>{Ok(())}
/// }
/// ```
/// This produces vector of u8 bytes, that looks like below
/// {\"time\":\"2021-04-08 00:00:00 +05:00\",\"temperature\":25,\"location\":{\"lati\":2100.4,\"alti\":2100.5,\"longi\":2100.4}}"
pub trait GroupedMeasurementVisitor {
    type Error;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;
    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;
    fn end_group(&mut self) -> Result<(), Self::Error>;
}
