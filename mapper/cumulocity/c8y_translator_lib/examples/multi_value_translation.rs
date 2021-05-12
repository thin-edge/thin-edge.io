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

    println!("\nThin_Edge_Json: \n{:#}", multi_value_thin_edge_json);
    let output = from_thin_edge_json(multi_value_thin_edge_json.as_bytes());
    match output {
        Ok(vec) => {
            println!("{}", String::from_utf8(vec).unwrap());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}

pub fn main() {
    multi_value_translation();
}
