use std::collections::HashMap;
use tedge_actors::Message;
use time::OffsetDateTime;

#[derive(Clone, Debug)]
pub struct MeasurementGroup {
    pub timestamp: Option<OffsetDateTime>,
    pub values: HashMap<String, Measurement>,
}

#[derive(Clone, Debug)]
pub enum Measurement {
    Single(f64),
    Multi(HashMap<String, f64>),
}

impl Message for Measurement {}
impl Message for MeasurementGroup {}
