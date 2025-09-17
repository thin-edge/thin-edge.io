use anyhow::Context;
use tedge_flows::database::FjallMeaDb;
use tedge_flows::database::MeaDb;
use tedge_flows::flow::DateTime;
use tedge_flows::flow::Message;

const DB_PATH: &str = "/etc/tedge/tedge-gen.db";

type Record = (DateTime, Message);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args();
    let series_name = args.nth(1).unwrap_or("latest-data-points".to_string());
    println!("Reading series name: {series_name}");

    let mut db = FjallMeaDb::open(DB_PATH)
        .await
        .with_context(|| format!("Failed to open DB at path={DB_PATH}"))?;

    let items = db
        .query_all(&series_name)
        .await
        .context("Failed to query for all items")?;

    items.iter().for_each(print_record);

    Ok(())
}

fn print_record(record: &Record) {
    let time = record.0.seconds;
    let topic = &record.1.topic;
    let payload = std::str::from_utf8(&record.1.payload).unwrap_or("<binary payload>");
    println!("[{time}]\t{topic}\t{payload}")
}
