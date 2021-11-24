use c8y_translator::json::from_thin_edge_json;

fn single_value_translation() {
    let single_value_thin_edge_json = r#"  {
    "temperature": 23,
    "pressure": 220
   }"#;

    println!("Thin_Edge_Json: \n{:#}", single_value_thin_edge_json);

    let output = from_thin_edge_json(single_value_thin_edge_json);
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
    single_value_translation();
}
