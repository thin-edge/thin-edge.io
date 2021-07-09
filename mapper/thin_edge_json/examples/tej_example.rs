use clock::{Clock, WallClock};
use thin_edge_json::group::MeasurementGrouper;
use thin_edge_json::measurement::*;
use thin_edge_json::serialize::ThinEdgeJsonSerializer;

fn tej_build_serialize() -> anyhow::Result<()> {
    //Produce the TEJ from raw data
    let mut grp_msg = MeasurementGrouper::new();

    grp_msg.visit_timestamp(WallClock.now())?;
    grp_msg.visit_measurement("temperature", 25.0)?;
    grp_msg.visit_start_group("location")?;
    grp_msg.visit_measurement("alti", 2100.4)?;
    grp_msg.visit_measurement("longi", 2100.4)?;
    grp_msg.visit_measurement("lati", 2100.4)?;
    grp_msg.visit_measurement("alti", 2100.5)?;
    grp_msg.visit_end_group()?;

    println!("Deserialized Tej=> {:#?}", grp_msg);

    //Serialize the TEJ to u8 bytes
    let mut visitor = ThinEdgeJsonSerializer::new();
    let group = grp_msg.end()?;
    group.accept(&mut visitor)?;

    println!("Serialized Tej=> {:?}", visitor.into_string()?);
    Ok(())
}
fn main() -> anyhow::Result<()> {
    tej_build_serialize()?;
    Ok(())
}
