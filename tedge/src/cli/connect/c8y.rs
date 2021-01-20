use crate::command::Command;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        String::new()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        // let config = ConnectCmd::read_configuration();
        // match self {
        //     ConnectCmd::Create {
        //         id,
        //         cert_path,
        //         key_path,
        //     } => {} // create_test_certificate(&config, id, cert_path, key_path)?,
        //     _ => {
        //         unimplemented!("{:?}", self);
        //     }
        // }
        Ok(())
    }
}
