use crate::command::Command;
use structopt::StructOpt;

mod c8y;

#[derive(StructOpt, Debug)]
pub enum ConnectCmd {
    /// Create connection to Cumulocity
    C8y(c8y::Connect),
}

impl ConnectCmd {
    fn sub_command(&self) -> &dyn Command {
        match self {
            ConnectCmd::C8y(ref cmd) => cmd,
        }
    }
}

impl Command for ConnectCmd {
    fn to_string(&self) -> String {
        self.sub_command().to_string()
    }

    fn run(&self, verbose: u8) -> Result<(), anyhow::Error> {
        self.sub_command().run(verbose)
    }
}

// #[derive(StructOpt, Debug)]
// pub enum ConnectCmd {
//     /// Create a device certificate
//     Connect {
//         /// The device identifier
//         #[structopt(long, default_value = "c8y", possible_values = &["c8y"])]
//         endpoint: c8y::C8y,
//     },
// }

// impl Command for ConnectCmd {
//     fn to_string(&self) -> String {
//         match self {
//             ConnectCmd::Connect { endpoint } => {
//                 format!("create a test certificate for the device {}.", endpoint)
//             }
//         }
//     }

//     fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
//         // let config = ConnectCmd::read_configuration();
//         // match self {
//         //     ConnectCmd::Create {
//         //         id,
//         //         cert_path,
//         //         key_path,
//         //     } => {} // create_test_certificate(&config, id, cert_path, key_path)?,
//         //     _ => {
//         //         unimplemented!("{:?}", self);
//         //     }
//         // }
//         Ok(())
//     }
// }
