use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = tedge_write::Args::parse();
    tedge_write::run(args)
}
