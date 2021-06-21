use clock::{Clock, WallClock};
use thin_edge_json::group::MeasurementGrouper;
use thin_edge_json::measurement::*;
use thin_edge_json::serialize::ThinEdgeJsonSerializer;

fn tej_build_serialize() -> anyhow::Result<()> {
    //Produce the TEJ from raw data
    let mut grp_msg = MeasurementGrouper::new();

    grp_msg.timestamp(&WallClock.now())?;
    grp_msg.measurement(None, "temperature", 25.0)?;
    grp_msg.measurement(Some("location"), "alti", 2100.4)?;
    grp_msg.measurement(Some("location"), "longi", 2100.4)?;
    grp_msg.measurement(Some("location"), "lati", 2100.4)?;
    grp_msg.measurement(Some("location"), "alti", 2100.5)?;

    println!("Deserialized Tej=> {:#?}", grp_msg);

    //Serialize the TEJ to u8 bytes
    let mut visitor = ThinEdgeJsonSerializer::new();
    grp_msg.accept(&mut visitor)?;

    println!("Serialized Tej=> {:?}", visitor.into_string()?);
    Ok(())
}
fn main() -> anyhow::Result<()> {
    tej_build_serialize()?;
    Ok(())
}
