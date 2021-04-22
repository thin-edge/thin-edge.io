use chrono::offset::FixedOffset;
use chrono::DateTime;
/// The FlatMeasurementVisitor trait is used to group the measurements from descrete measurements.
/// The descrete measurements might belong to a group or individual
/// All these are grouped as one ThinEdge Json message
/// For example the MeasurementGrouper implementing FlatMeasurementVisitor to group
/// the messages
/// ```
/// impl FlatMeasurementVisitor for MeasurementGrouper {
///     fn timestamp(&mut self, timestamp: &DateTime<FixedOffset>) -> Result<(), Self::Error> {}
///     fn measurement(&mut self,group: Option<&str>,name: &str,value: f64,) -> Result<(), Self::Error> {}
/// }
///
/// let mut grp_msg = MeasurementGrouper::new(test_timestamp());
/// grp_msg.timestamp(&test_timestamp())?;
/// measurement that does not belong to any group
/// grp_msg.measurement(None, "temperature", 25.0)?;
/// measurement that belongs to a group
/// grp_msg.measurement(Some("location"), "alti", 2100.4)?;
/// grp_msg.measurement(Some("location"), "longi", 2100.4)?;
/// grp_msg.measurement(Some("location"), "lati", 2100.4)?;
/// grp_msg.measurement(Some("location"), "alti", 2100.5)?;
///
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
/// ```

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
/// trait, to serialize the Thinedge Json message.
/// ```
/// impl GroupedMeasurementVisitor for ThinEdgeJsonSerializer {
///      
/// }
/// ```
/// This produces vector of u8 bytes
pub trait GroupedMeasurementVisitor {
    type Error;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error>;
    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error>;
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;
    fn end_group(&mut self) -> Result<(), Self::Error>;
}
