use c8y_translator_lib::json::from_thin_edge_json;

fn thin_edge_translation_with_type_and_timestamp() {
    let single_value_thin_edge_json_with_type_and_time = r#"   {
     "time" : "2013-06-22T17:03:14.100+02:00",
     "temperature": 23,
     "pressure": 220
    }"#;

    println!(
        "\nThin_Edge_Json: \n{:#}",
        single_value_thin_edge_json_with_type_and_time
    );
    let output = from_thin_edge_json(single_value_thin_edge_json_with_type_and_time.as_bytes());
    match output {
        Ok(vec) => {
            println!("{}", vec);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}

pub fn main() {
    thin_edge_translation_with_type_and_timestamp();
}
