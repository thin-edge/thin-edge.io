use anyhow::Context;
use cfg_if::cfg_if;
use tedge_flows::database;
use tedge_flows::database::MeaDb;
use tedge_flows::flow::DateTime;
use tedge_flows::flow::Message;

const DEFAULT_DB_PATH: &str = "/etc/tedge/tedge-flows.db";

type Record = (DateTime, Message);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args();
    let _program_name = args.next();

    // Parse arguments: [series_name] [db_path] [--backend=fjall|sqlite]
    let series_name = args.next().unwrap_or("latest-data-points".to_string());
    let db_path = args.next().unwrap_or(DEFAULT_DB_PATH.to_string());
    let backend = args.next().unwrap_or_else(|| {
        cfg_if! {
            if #[cfg(feature = "fjall-db")] {
                "fjall".to_string()
            } else if #[cfg(feature = "sqlite-db")] {
                "sqlite".to_string()
            } else {
                compile_error!(
                    "No database backend enabled. Enable either 'fjall-db' or 'sqlite-db' feature."
                );
            }
        }
    });

    println!("Reading series: {series_name} from {db_path} using {backend} backend");

    let mut db: Box<dyn MeaDb> = match backend.as_str() {
        #[cfg(feature = "fjall-db")]
        "fjall" => Box::new(
            database::FjallMeaDb::open(&db_path)
                .await
                .with_context(|| format!("Failed to open Fjall DB at path={db_path}"))?,
        ),

        #[cfg(feature = "sqlite-db")]
        "sqlite" => Box::new(
            database::SqliteMeaDb::open(&db_path)
                .await
                .with_context(|| format!("Failed to open SQLite DB at path={db_path}"))?,
        ),

        _ => {
            eprintln!("Invalid backend: {backend}. Available backends:");
            #[cfg(feature = "fjall-db")]
            eprintln!("  - fjall");
            #[cfg(feature = "sqlite-db")]
            eprintln!("  - sqlite");
            std::process::exit(1);
        }
    };

    let items = db
        .query_all(&series_name)
        .await
        .context("Failed to query for all items")?;

    items.iter().for_each(print_record);

    Ok(())
}

fn print_record(record: &Record) {
    let time = record.0.seconds;
    let nanos = record.0.nanoseconds;
    let topic = &record.1.topic;
    let payload = std::str::from_utf8(&record.1.payload).unwrap_or("<binary payload>");
    println!("[{time}.{nanos}]\t{topic}\t{payload}")
}
