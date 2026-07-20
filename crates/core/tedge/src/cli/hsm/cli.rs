use tedge_config::TEdgeConfig;

use super::create_key::CreateKeyArgs;
use super::init_token::InitArgs;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeHsmCli {
    /// Initialize a PKCS #11 token so that it can be used to store keys.
    ///
    /// This performs the full PKCS #11 token initialization: it sets the token label and the
    /// Security Officer (SO) PIN, then sets the user PIN used by all other operations. Afterwards
    /// the token can hold keys, e.g. created via `tedge hsm create-key`.
    ///
    /// The slot to initialize is auto-discovered when `--slot` is not given: the single slot
    /// holding an uninitialized token is selected. The command is idempotent: if a token with the
    /// requested label is already initialized, it is left untouched.
    ///
    /// The resulting token URI is printed to stdout, so it can be captured in scripts:
    /// `URI=$(tedge hsm init)`.
    Init(InitArgs),

    /// Generate a new keypair on the PKCS #11 token and select it to be used.
    ///
    /// Can be used to generate a keypair on the TOKEN. If the TOKEN argument is not provided, the
    /// command auto-discovers the token to use: if no initialized token exists yet, an
    /// uninitialized slot is initialized automatically; if exactly one initialized token exists, it
    /// is used; if several exist, the available tokens are printed so one can be selected.
    ///
    /// The command generates an RSA or an ECDSA keypair on the token. When using RSA, `--bits` is
    /// used to set the size of the key, when using ECDSA, `--curve` is used.
    ///
    /// The command is idempotent: if a key matching the given label (and id, if provided) already
    /// exists on the token, it is reused instead of creating a duplicate. Pass `--force-new` to
    /// always generate a new key.
    ///
    /// After the key is generated (or reused), tedge config is updated to use the key using the
    /// `device.key_uri` property. Depending on the selected cloud, we use `device.key_uri` setting
    /// for that cloud, e.g. `create-key c8y` will write to `c8y.device.key_uri`.
    CreateKey(CreateKeyArgs),
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeHsmCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeHsmCli::Init(args) => args.build_command(config),
            TEdgeHsmCli::CreateKey(args) => args.build_command(config),
        }
    }
}
