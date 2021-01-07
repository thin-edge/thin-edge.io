use c8y_json_translator::CumulocityJson;

fn single_value_translation() {
    let single_value_thin_edge_json = r#"  {
    "temperature": 23,
    "pressure": 220
   }"#;

    println!("Thin_Edge_Json: \n{:#}", single_value_thin_edge_json);

    println!(
        "\nc8yjson: \n {}",
        CumulocityJson::from_thin_edge_json(
            &String::from(single_value_thin_edge_json).into_bytes(),
        )
    );
}

fn multi_value_translation() {
    let multi_value_thin_edge_json = r#"   {
      "temperature": 0 ,
      "location": {
        "latitude": 32.54,
        "longitude": -117.67,
        "altitude": 98.6
      },
      "pressure": 98
   }"#;

    println!("\nThin_Edge_Json: \n{:#}", multi_value_thin_edge_json);
    println!(
        "\nc8yjson: \n {}",
        CumulocityJson::from_thin_edge_json(&String::from(multi_value_thin_edge_json).into_bytes(),)
    );
}

fn thin_edge_translation_with_type_and_time_stamp() {
    let single_value_thin_edge_json_with_type_and_time = r#"   {
     "time" : "2013-06-22T17:03:14.000+02:00",
     "temperature": 23,
     "pressure": 220
    }"#;

    println!(
        "\nThin_Edge_Json: \n{:#}",
        single_value_thin_edge_json_with_type_and_time
    );
    println!(
        "\nc8yjson: \n {}",
        CumulocityJson::from_thin_edge_json(
            &String::from(single_value_thin_edge_json_with_type_and_time).into_bytes(),
        )
    );
}

pub fn main() {
    single_value_translation();
    multi_value_translation();
    thin_edge_translation_with_type_and_time_stamp();
}
