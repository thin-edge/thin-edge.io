use super::command::Cmd;
use std::error::Error;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum CertCmd {
    /// Create a device certificate
    Create {
        /// The device identifier
        #[structopt(long)]
        id: String,

        /// The path where the device certificate will be stored
        #[structopt(long, default_value = "./tedge-certificate.pem")]
        cert_path: String,

        /// The path where the device private key will be stored
        #[structopt(long, default_value = "./tedge-private-key.pem")]
        key_path: String,
    },

    /// Show the device certificate, if any
    Show,

    /// Remove the device certificate
    Remove,
}

impl Cmd for CertCmd {
    fn run(&self, _verbose: u8) -> Result<(), Box<dyn Error>> {
        println!("{:?}", self);
        Ok(())
    }
}
