use env_logger::Env;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    log::info!("tedge-mapper starting!");
}
