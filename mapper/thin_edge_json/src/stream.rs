use chrono::offset::FixedOffset;
use chrono::DateTime;

/// We avoid two kinds of "visitors" (Flat vs. Group). That's a source of
/// lots of confusion for no good reason.
///
/// A "flat" visitor can be represented on the "grouped" visitor by
/// a sequence of (StartGroup(group), Measurement, EndGroup).
///
/// These kind of things should be part of the implementation and NOT the trait.
pub enum MeasurementStreamItem<'a> {
    /// Marks the beginning of the stream / document.
    StartDocument,
    /// Marks the end of the stream / document.
    EndDocument,
    /// A timestamp
    Timestamp(DateTime<FixedOffset>),
    /// Marks the start of a measurement group.
    StartGroup(&'a str),
    /// Marks the end of a measurement group.
    EndGroup,
    /// A measurement
    Measurement { name: &'a str, value: f64 },
}

/// A measurement stream consumer / sink / processor.
pub trait MeasurementStreamConsumer: std::fmt::Debug {
    type Error: std::error::Error + std::fmt::Debug;

    fn consume<'a>(&mut self, item: MeasurementStreamItem<'a>) -> Result<(), Self::Error>;
}

/// Provides methods to send MeasurementStreamItem's to a stream consumer.
#[derive(Debug)]
pub struct StreamBuilder<E, T>(T)
where
    E: std::error::Error + std::fmt::Debug,
    T: MeasurementStreamConsumer<Error = E>;

impl<E, T> From<T> for StreamBuilder<E, T>
where
    E: std::error::Error + std::fmt::Debug,
    T: MeasurementStreamConsumer<Error = E>,
{
    fn from(t: T) -> Self {
        Self(t)
    }
}

impl<E, T> StreamBuilder<E, T>
where
    E: std::error::Error + std::fmt::Debug,
    T: MeasurementStreamConsumer<Error = E>,
{
    pub fn start(&mut self) -> Result<(), E> {
        self.0.consume(MeasurementStreamItem::StartDocument)
    }
    pub fn end(&mut self) -> Result<(), E> {
        self.0.consume(MeasurementStreamItem::EndDocument)
    }
    pub fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), E> {
        self.0.consume(MeasurementStreamItem::Timestamp(timestamp))
    }
    pub fn start_group(&mut self, group: &str) -> Result<(), E> {
        self.0.consume(MeasurementStreamItem::StartGroup(group))
    }
    pub fn end_group(&mut self) -> Result<(), E> {
        self.0.consume(MeasurementStreamItem::EndGroup)
    }
    pub fn measurement(&mut self, name: &str, value: f64) -> Result<(), E> {
        self.0
            .consume(MeasurementStreamItem::Measurement { name, value })
    }
    /// This is a convenience method on the **builder**!!!
    pub fn measurement_in_group(&mut self, group: &str, name: &str, value: f64) -> Result<(), E> {
        self.start_group(group)
            .and_then(|()| self.measurement(name, value))
            .and_then(|()| self.end_group())
    }
    pub fn inner(self) -> T {
        self.0
    }
}
