use std::collections::HashMap;

//mod tej_serializer;
pub trait MeasurementConsumer {
    type Error;
    type Data;
    fn start(&mut self) -> Result<(), Self::Error>;
    fn end(&mut self) -> Result<(), Self::Error>;
    fn timestamp(&mut self, value: String) -> Result<(), Self::Error>;
    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error>;
    fn start_group(&mut self, group: &str) -> Result<(), Self::Error>;
    fn end_group(&mut self) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub struct MeasurementMap {
    pub timestamp: String,
    pub values: HashMap<String, Measurement>,
}
#[derive(Debug)]
pub enum Measurement {
    Single(f64),
    Multi(HashMap<String, f64>),
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonMapError {
    #[error("Duplicated measurement: {0}")]
    DuplicatedMeasurement(String),

    #[error("Duplicated measurement: {0}.{1}")]
    DuplicatedSubMeasurement(String, String),
}

impl MeasurementMap {
    fn new() -> Self {
        Self {
            timestamp: "4-20-2021".into(),
            values: HashMap::new(),
        }
    }

    fn serialize_tej(&self) -> Vec<u8> {
        let mut t_serializer = ThinEdgeJsonSerializer::new();
        t_serializer.start();
        t_serializer.timestamp(self.timestamp.clone());
        for (key, value) in self.values.iter() {
            match value {
                Measurement::Single(sv) => {
                    t_serializer.measurement(None, key, *sv);
                }
                Measurement::Multi(m) => {
                    t_serializer.start_group(key);
                    for (key, value) in m.iter() {
                        t_serializer.measurement(None, key, *value);
                    }
                    t_serializer.end_group();
                }
            }
        }
        //Prints the serialized TEJ Message
        t_serializer.end();
        t_serializer.tej_serailized()
    }
}

impl MeasurementConsumer for MeasurementMap {
    type Error = ThinEdgeJsonMapError;
    type Data = MeasurementMap;

    fn start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn timestamp(&mut self, value: String) -> Result<(), Self::Error> {
        self.timestamp = value;
        Ok(())
    }

    fn measurement(
        &mut self,
        group: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error> {
        let key = name.to_owned();

        match group {
            None => {
                self.values.insert(key, Measurement::Single(value));
                return Ok(());
            }
            Some(group) => {
                let key = group.to_owned();

                if !self.values.contains_key(&key) {
                    self.values
                        .insert(key.clone(), Measurement::Multi(HashMap::new()));
                }

                let group = match self.values.get_mut(&key) {
                    Some(Measurement::Multi(group)) => group,
                    _ => {
                        return Err(ThinEdgeJsonMapError::DuplicatedMeasurement(key));
                    }
                };

                let sub_key = name.to_owned();
                group.insert(sub_key, value);
                Ok(())
            }
        }
    }

    fn start_group(&mut self, _group: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

use std::io::Write;

/// Serialize a series of measurements into a ThinEdgeJson byte-string.
/// Perform no check beyond the fact that groups are properly closed.
pub struct ThinEdgeJsonSerializer {
    buffer: Vec<u8>,
    is_within_group: bool,
    needs_separator: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonSerializationError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementStreamError {
    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("Unexpected end of data")]
    UnexpectedEndOfData,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,
}

impl ThinEdgeJsonSerializer {
    pub fn new() -> Self {
        ThinEdgeJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: false,
        }
    }
    fn tej_serailized(&mut self) -> Vec<u8> {
        self.buffer.clone()
    }
}

impl MeasurementConsumer for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;
    type Data = Vec<u8>;
    fn start(&mut self) -> Result<(), Self::Error> {
        self.buffer.push(b'{');
        self.needs_separator = false;
        Ok(())
    }

    fn end(&mut self) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        self.buffer.push(b'}');
        //println!("vector {:#?}", std::str::from_utf8(&self.buffer));
        Ok(())
    }

    fn timestamp(&mut self, value: String) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"time\":\"{}\"", value))?;
        self.needs_separator = true;
        Ok(())
    }

    fn measurement(
        &mut self,
        _grpname: Option<&str>,
        name: &str,
        value: f64,
    ) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"{}\":{}", name, value))?;
        self.needs_separator = true;
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"{}\":{{", group))?;
        self.needs_separator = false;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.buffer.push(b'}');
        self.needs_separator = true;
        Ok(())
    }
}

fn main() {
    //Produce the TEJ from raw data
    let mut tej_producer = MeasurementMap::new();
    tej_producer.timestamp("4-20-2020".into()).unwrap();
    tej_producer.measurement(None, "temperature", 25.0).unwrap();
    tej_producer
        .measurement(Some("location"), "alti", 2100.4)
        .unwrap();
    tej_producer
        .measurement(Some("location"), "longi", 2100.4)
        .unwrap();
    tej_producer
        .measurement(Some("location"), "lati", 2100.4)
        .unwrap();
    tej_producer
        .measurement(Some("location"), "alti", 2100.5)
        .unwrap();

    println!("Deserialized Tej=> {:#?}", tej_producer);

    //Serialize the TEJ to u8 bytes    
    println!("Serialized Tej=> {:?}", std::str::from_utf8(&tej_producer.serialize_tej()));
}
