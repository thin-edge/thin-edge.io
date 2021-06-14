use c8y_translator_lib::json::from_thin_edge_json;

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

    println!("\nThin_Edge_Json: {:#}", multi_value_thin_edge_json);
    let output = from_thin_edge_json(multi_value_thin_edge_json);
    match output {
        Ok(vec) => {
            println!("{:?}", vec);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}

pub fn main() {
    multi_value_translation();
}
