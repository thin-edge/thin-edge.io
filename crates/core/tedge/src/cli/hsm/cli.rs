use tedge_config::TEdgeConfig;

use super::change_pin::ChangePinArgs;
use super::create_key::CreateKeyArgs;
use super::delete_key::DeleteKeyArgs;
use super::init_token::InitArgs;
use super::list_keys::ListKeysArgs;
use super::list_tokens::ListTokensArgs;
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
    /// The command is idempotent: if a token with the requested label is already initialized, it
    /// is left untouched and its URI is returned. The slot to initialize is auto-discovered when
    /// no URI is given: the single slot holding an uninitialized token is selected. When a URI is
    /// given, it must match a token, otherwise the command fails.
    ///
    /// The resulting token URI is printed to stdout, so it can be captured in scripts:
    /// `URI=$(tedge hsm init)`.
    Init(InitArgs),

    /// List the PKCS #11 tokens available to the HSM.
    ///
    /// Every slot that holds a token is listed with its slot id, label, initialization state, and
    /// its PKCS #11 URI. Both initialized and uninitialized tokens are shown, so a slot waiting for
    /// `tedge hsm init` is visible too. The printed URI can be passed to other commands, e.g. to
    /// select a specific token with `tedge hsm create-key <URI>`.
    ///
    /// Only public token metadata is read, so no PIN is required.
    ListTokens(ListTokensArgs),

    /// List the keys stored on a PKCS #11 token.
    ///
    /// Both private and public key objects are listed, each with its label, id, type and full
    /// PKCS #11 URI. The URI can be used as `device.key_uri` to select an existing key, e.g. one
    /// pre-provisioned on the token.
    ///
    /// The token is auto-discovered when no URI is given. Listing private keys requires a login, so
    /// the configured PIN (or `--pin`) is used.
    ListKeys(ListKeysArgs),

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

    /// Change or reset the user PIN of a PKCS #11 token.
    ///
    /// By default the current user PIN is changed to a new one. The token is auto-discovered when
    /// no URI is given: the single initialized token is selected; if several exist, pass a PKCS #11
    /// URI that identifies one.
    ///
    /// If the token's current user PIN is unknown or the token is locked out, pass `--reset` along
    /// with the Security Officer PIN to reset the user PIN instead.
    ///
    /// On success, `device.cryptoki.pin` in tedge config is updated to the new PIN so the PKCS #11
    /// provider keeps working. Restart tedge-p11-server afterwards for the change to take effect.
    ///
    /// PINs not passed as flags are prompted for interactively, keeping them out of the shell
    /// history.
    ChangePin(ChangePinArgs),

    /// Delete a key from a PKCS #11 token.
    ///
    /// The key is selected by `--label` and/or `--id` (as used with `create-key`), or by a full key
    /// URI. Both the private and public key objects sharing the label/id are destroyed.
    ///
    /// This is destructive and irreversible. Unless `--force` is given, the command prompts for
    /// confirmation and refuses to delete a key that is referenced by the device's configured
    /// `key_uri`, since that would break the cloud connection.
    DeleteKey(DeleteKeyArgs),
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeHsmCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            TEdgeHsmCli::Init(args) => args.build_command(config),
            TEdgeHsmCli::ListTokens(args) => args.build_command(config),
            TEdgeHsmCli::ListKeys(args) => args.build_command(config),
            TEdgeHsmCli::CreateKey(args) => args.build_command(config),
            TEdgeHsmCli::ChangePin(args) => args.build_command(config),
            TEdgeHsmCli::DeleteKey(args) => args.build_command(config),
        }
    }
}
