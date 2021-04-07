pub mod builder;
pub mod c8y_json_serializer;
pub mod error;
pub mod visitor;

pub use self::{builder::*, c8y_json_serializer::*, error::*, visitor::*};

fn main() -> Result<(), MeasurementError> {
    use chrono::{FixedOffset, TimeZone};

    let mut c8y_json_serializer = C8yJsonSerializer::new(
        "my_measurement".into(),
        FixedOffset::east(5 * 3600)
            .ymd(2016, 11, 08)
            .and_hms(0, 0, 0),
    );

    let mut builder = MeasurementBuilder::new(&mut c8y_json_serializer);

    builder = builder
        .start()?
        .measurement_type("c8y")?
        // A single value measurement
        .measurement_data("abc", 123.3)?
        .measurement_data("temp", 333.4)?
        .start_group("location")?
        .measurement_data("x", 123.0)?
        .measurement_data("y", 123.0)?
        .measurement_data("z", 123.0)?
        .end_group()?;

    builder = builder.start_group("loop")?;

    for i in 1..10 {
        builder = builder.measurement_data(&format!("k_{}", i), (i * 13) as f64)?;
    }

    let () = builder.end_group()?.end()?;

    println!("{}", String::from_utf8(c8y_json_serializer.data()).unwrap());

    Ok(())
}
