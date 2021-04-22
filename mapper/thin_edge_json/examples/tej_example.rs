use chrono::offset::FixedOffset;
use chrono::DateTime;
use chrono::TimeZone;
use thin_edge_json::{group::MeasurementGrouper, serialize::ThinEdgeJsonSerializer};
use thin_edge_json::{group::MeasurementGrouperError, measurement::FlatMeasurementVisitor};

fn test_timestamp() -> DateTime<FixedOffset> {
    FixedOffset::east(5 * 3600)
        .ymd(2021, 04, 08)
        .and_hms(0, 0, 0)
}

fn tej_build_serialize() -> Result<(), MeasurementGrouperError> {
    //Produce the TEJ from raw data
    let mut grp_msg = MeasurementGrouper::new(test_timestamp());

    grp_msg.timestamp(&test_timestamp())?;
    grp_msg.measurement(None, "temperature", 25.0)?;
    grp_msg.measurement(Some("location"), "alti", 2100.4)?;
    grp_msg.measurement(Some("location"), "longi", 2100.4)?;
    grp_msg.measurement(Some("location"), "lati", 2100.4)?;
    grp_msg.measurement(Some("location"), "alti", 2100.5)?;

    println!("Deserialized Tej=> {:#?}", grp_msg);

    let mut visitor = ThinEdgeJsonSerializer::new();
    grp_msg.accept(&mut visitor)?;

    //Serialize the TEJ to u8 bytes
    let bytes = visitor.bytes()?;
    println!("Serialized Tej=> {:?}", std::str::from_utf8(&bytes));
    Ok(())
}
fn main() -> anyhow::Result<()> {
    tej_build_serialize()?;
    Ok(())
}
