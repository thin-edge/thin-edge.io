use chrono::offset::FixedOffset;
use chrono::DateTime;
use chrono::TimeZone;
use thin_edge_json::measurement::FlatMeasurementVisitor;
use thin_edge_json::{group::MeasurementMap, serialize::ThinEdgeJsonSerializer};

fn test_timestamp() -> DateTime<FixedOffset> {
    FixedOffset::east(5 * 3600)
        .ymd(2021, 04, 08)
        .and_hms(0, 0, 0)
}

fn tej_build_serialize() {
    //Produce the TEJ from raw data
    let mut grp_msg = MeasurementMap::new(test_timestamp());

    grp_msg.timestamp(test_timestamp()).unwrap();
    grp_msg.measurement(None, "temperature", 25.0).unwrap();
    grp_msg
        .measurement(Some("location"), "alti", 2100.4)
        .unwrap();
    grp_msg
        .measurement(Some("location"), "longi", 2100.4)
        .unwrap();
    grp_msg
        .measurement(Some("location"), "lati", 2100.4)
        .unwrap();
    grp_msg
        .measurement(Some("location"), "alti", 2100.5)
        .unwrap();

    println!("Deserialized Tej=> {:#?}", grp_msg);

    let mut visitor = ThinEdgeJsonSerializer::new();
    visitor.start().unwrap();
    grp_msg.accept(&mut visitor);
    //Serialize the TEJ to u8 bytes
    let bytes = visitor.get_searialized_tej();
    visitor.end().unwrap();
    println!("Serialized Tej=> {:?}", std::str::from_utf8(&bytes));
}
fn main() {
    tej_build_serialize();
}
