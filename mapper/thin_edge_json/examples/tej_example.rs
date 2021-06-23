use clock::{Clock, WallClock};
use thin_edge_json::serializer::ThinEdgeJsonSerializer;
use thin_edge_json::stream::*;

fn tej_build_serialize() -> anyhow::Result<()> {
    //Produce the TEJ from raw data
    let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());

    serializer.start()?;
    serializer.timestamp(WallClock.now())?;
    serializer.measurement("temperature", 25.0)?;
    serializer.measurement_in_group("location", "alti", 2100.4)?;
    serializer.measurement_in_group("location", "longi", 2100.4)?;
    serializer.measurement_in_group("location", "lati", 2100.4)?;
    serializer.measurement_in_group("location", "alti", 2100.5)?;
    serializer.end()?;

    // Serialize the TEJ to u8 bytes

    println!("Serialized Tej=> {:?}", serializer.inner().into_string()?);
    Ok(())
}
fn main() -> anyhow::Result<()> {
    tej_build_serialize()?;
    Ok(())
}
