pub trait Cmd {
    fn run(&self, verbose: u8) -> Result<(), Box<dyn std::error::Error>>;
}
