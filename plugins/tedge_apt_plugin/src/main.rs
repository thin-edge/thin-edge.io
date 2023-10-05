use clap::Parser;

fn main() {
    let cli = tedge_apt_plugin::AptCli::try_parse();
    tedge_apt_plugin::run_and_exit(cli);
}
