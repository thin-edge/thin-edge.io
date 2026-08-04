use anyhow::Context;
use camino::Utf8Path;
use clap::ValueEnum;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_p11::pkcs11::CreateKeyParams;
use tedge_p11::pkcs11::KeyTypeParams;
use tedge_p11::service::CreateKeyRequest;
use tedge_p11::service::InitTokenRequest;
use tedge_p11::CryptokiConfig;
use tedge_p11::SecretString;
use tracing::warn;

use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::ConfigError;
use tedge_config::tedge_toml::CloudConfig;

/// Arguments of the key-generation command.
///
/// Shared between `tedge hsm create-key` and the deprecated `tedge cert create-key-hsm` alias, so
/// both accept exactly the same options.
#[derive(Debug, clap::Args)]
pub struct CreateKeyArgs {
    /// Human readable description (CKA_LABEL attribute) for the key.
    #[arg(long, default_value = "tedge")]
    pub label: String,

    /// Key identifier for the keypair (CKA_ID attribute).
    ///
    /// If provided and no object exists on the token with the same ID, this will be the ID of
    /// the new keypair. If an object with this ID already exists, the operation will return an
    /// error. If not provided, a random ID will be generated and used by the keypair.
    ///
    /// The id shall be provided as hex digits without `0x` prefix, e.g. `--id 010203`. Byte
    /// separators as printed by other tools (e.g. `01:02:03`) are accepted as well.
    #[arg(long)]
    pub id: Option<String>,

    /// The type of the key.
    #[arg(long, default_value = "ecdsa")]
    pub r#type: KeyType,

    /// The size of the RSA keys in bits. Should only be used with --type rsa.
    #[arg(long, default_value = "2048", group = "key_params")]
    pub bits: RsaBits,

    /// The curve (size) of the ECDSA key. Should only be used with --type ecdsa.
    #[arg(long, default_value = "p256", group = "key_params")]
    pub curve: EcCurve,

    /// User PIN value for logging into the PKCS #11 token.
    ///
    /// This flag can be used to provide a PIN when creating a new key without needing to update
    /// tedge-config, which can be helpful when initializing keys on new tokens.
    ///
    /// Note that in contrast to the URI of the key, which will be written to tedge-config
    /// automatically when the keypair is created, PIN will not be written automatically and may
    /// be needed to written manually using tedge config set (if not using tedge-p11-server with
    /// the correct default PIN).
    #[arg(long)]
    pub pin: Option<String>,

    /// Security Officer (SO) PIN used if a token has to be initialized automatically.
    ///
    /// When no initialized token exists, an uninitialized slot is initialized automatically
    /// before the key is created. The SO PIN is required to initialize a token; if not
    /// provided, the user PIN is used as the SO PIN (which works for tokens that do not enforce
    /// distinct PINs, such as SoftHSM2).
    #[arg(long)]
    pub so_pin: Option<String>,

    /// Label (CKA_LABEL) to assign to the token if one has to be initialized automatically.
    #[arg(long, default_value = "tedge")]
    pub token_label: String,

    /// Always create a new key, even if one with the same label already exists.
    ///
    /// By default the command is idempotent: if a key matching the given label (and id, if
    /// provided) already exists on the token, it is reused instead of creating a duplicate.
    /// Pass this flag to force generating a new key regardless.
    #[arg(long, default_value_t = false)]
    pub force_new: bool,

    /// Path where public key will be saved when a keypair is generated.
    #[arg(long)]
    pub outfile_pubkey: Option<Box<Utf8Path>>,

    // can't document subcommands here because one would have to document variants of the enum
    // but this type is used in other places
    #[clap(subcommand)]
    pub cloud: Option<CloudArg>,

    /// The URI of the token where the keypair should be created.
    ///
    /// If this argument is missing, the token is auto-discovered: an uninitialized token is
    /// initialized automatically, a single initialized token is used as-is, and if several
    /// exist the available tokens are listed so one can be selected.
    pub token: Option<String>,
}

impl CreateKeyArgs {
    pub fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let cloud: Option<Cloud> = self.cloud.map(<_>::try_into).transpose()?;
        let cloud_config = match cloud.as_ref() {
            Some(c) => Some(config.as_cloud_config(c.into())?),
            None => None,
        };
        let cryptoki_config = config
            .device
            .cryptoki_config(cloud_config.as_ref().map(|c| &**c as &dyn CloudConfig))?
            .context("Cryptoki config is not enabled")?;

        Ok(CreateKeyHsmCmd {
            cryptoki_config,
            label: self.label,
            r#type: self.r#type,
            bits: self.bits,
            curve: self.curve,
            id: self.id,
            pin: self.pin,
            so_pin: self.so_pin,
            token_label: self.token_label,
            force_new: self.force_new,
            outfile_pubkey: self.outfile_pubkey,
            cloud,
            token: self.token,
        }
        .into_boxed())
    }
}

pub struct CreateKeyHsmCmd {
    pub cryptoki_config: CryptokiConfig,
    pub bits: RsaBits,
    pub curve: EcCurve,
    pub label: String,
    pub r#type: KeyType,
    pub id: Option<String>,
    pub pin: Option<String>,
    pub so_pin: Option<String>,
    pub token_label: String,
    pub force_new: bool,
    pub outfile_pubkey: Option<Box<Utf8Path>>,
    pub cloud: Option<Cloud>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KeyType {
    Rsa,
    Ecdsa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RsaBits {
    #[value(name = "2048")]
    Bits2048,
    #[value(name = "3072")]
    Bits3072,
    #[value(name = "4096")]
    Bits4096,
}

impl From<RsaBits> for u16 {
    fn from(value: RsaBits) -> Self {
        match value {
            RsaBits::Bits2048 => 2048,
            RsaBits::Bits3072 => 3072,
            RsaBits::Bits4096 => 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EcCurve {
    P256,
    P384,
}

impl From<EcCurve> for u16 {
    fn from(value: EcCurve) -> Self {
        match value {
            EcCurve::P256 => 256,
            EcCurve::P384 => 384,
        }
    }
}

#[async_trait::async_trait]
impl Command for CreateKeyHsmCmd {
    fn description(&self) -> String {
        "Generate a keypair.".into()
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.ensure_key(config).await?;
        Ok(())
    }
}

impl CreateKeyHsmCmd {
    /// Ensures a usable keypair exists on the token and returns its URI.
    ///
    /// This is the body of `tedge hsm create-key`, also reused by `tedge cert download` to create
    /// a key automatically when none exists yet: the token is auto-discovered (initializing it if
    /// needed), an existing key with the same label is reused, and the key URI is saved to tedge
    /// config.
    pub(crate) async fn ensure_key(&self, config: TEdgeConfig) -> anyhow::Result<String> {
        let key = match self.r#type {
            KeyType::Rsa => KeyTypeParams::Rsa {
                bits: self.bits.into(),
            },
            KeyType::Ecdsa => KeyTypeParams::Ec {
                curve: self.curve.into(),
            },
        };
        let id = self
            .id
            .as_ref()
            .map(|s| parse_id(s))
            .transpose()
            .context("invalid id")?;

        let cryptoki = tedge_p11::tedge_p11_service(self.cryptoki_config.clone())?;
        let token = match self.token.clone() {
            Some(token) => token,
            None => {
                let tokens = cryptoki.get_tokens_uris()?;
                match tokens.len() {
                    // No initialized token: auto-discover and initialize an uninitialized slot so
                    // the device can go from a bare module path to a usable key in one command.
                    0 => {
                        eprintln!(
                            "No initialized token was found; attempting to initialize one automatically."
                        );
                        let response = cryptoki.init_token(InitTokenRequest {
                            label: self.token_label.clone(),
                            so_pin: self.so_pin.clone().map(SecretString::from),
                            pin: self.pin.clone().map(SecretString::from),
                            uri: None,
                        })?;
                        eprintln!("Initialized a new token: {}", response.uri);
                        response.uri
                    }
                    // Exactly one initialized token: it is unambiguous, so use it.
                    1 => {
                        let token = tokens.into_iter().next().expect("length checked to be 1");
                        eprintln!("Using the only available token: {token}");
                        token
                    }
                    // Multiple tokens: the choice is ambiguous, ask the user to select one.
                    _ => {
                        anyhow::bail!(
                            "No token URL was provided for this operation; the available tokens are:\n{}",
                            tokens.join("\n")
                        );
                    }
                }
            }
        };

        // Unless --force-new is given, reuse an existing key with the same label (and id, if
        // provided) instead of creating a duplicate. This makes the command idempotent, which is
        // handy for re-runnable provisioning scripts.
        let existing = if self.force_new {
            None
        } else {
            let key_uri = existing_key_uri(&token, &self.label, id.as_deref());
            match cryptoki.get_public_key_pem(Some(&key_uri)) {
                Ok(pem) => Some((pem, key_uri)),
                Err(_) => None,
            }
        };

        let (pubkey_pem, uri) = match existing {
            Some((pubkey_pem, uri)) => {
                eprintln!(
                    "A key labelled '{}' already exists on the token; reusing it (pass --force-new to create a new key instead).",
                    self.label
                );
                (pubkey_pem, uri)
            }
            None => {
                let params = CreateKeyParams {
                    key,
                    label: self.label.clone(),
                    id,
                };

                // generate a keypair
                // should probably verify the keys before using them
                let key = cryptoki.create_key(CreateKeyRequest {
                    uri: token,
                    params,
                    pin: self.pin.clone().map(SecretString::from),
                })?;
                eprintln!("New keypair was successfully created.");
                (key.pem, key.uri)
            }
        };

        eprintln!("Key URI: {uri}");
        eprintln!("Public key:\n{pubkey_pem}\n");

        // Operations below may fail for some reason (e.g. no permissions to write to outfile), but
        // the key was still created, so we still consider the operation succeeded.
        if let Some(outfile) = &self.outfile_pubkey {
            let r = std::fs::write(outfile.as_ref(), pubkey_pem);
            if let Err(e) = r {
                warn!(?e, path=%outfile, "Failed to save the public key to file");
            }
        }

        if let Err(e) = save_key_uri_to_config(config, self.cloud.as_ref(), &uri, &*cryptoki).await
        {
            warn!(?e, "Failed to save public key URI to tedge-config. You may need to enter key URI in tedge-config manually to use the new key.")
        }

        Ok(uri)
    }
}

/// Saves the key URI to the `device.key_uri` config setting of the selected cloud, so the key is
/// used for subsequent connections.
///
/// An existing selection of a working key is never overwritten: if the setting already points to a
/// different key that still exists on the token, the config is left unchanged and the `tedge
/// config set` command to switch to the new key is printed instead. A URI pointing to a key that
/// no longer exists is overwritten (with a note), since keeping it would select nothing.
async fn save_key_uri_to_config(
    config: TEdgeConfig,
    cloud: Option<&Cloud>,
    uri: &str,
    cryptoki: &dyn tedge_p11::service::TedgeP11Service,
) -> anyhow::Result<()> {
    let key = extract_device_id_for_cloud(cloud)?;

    let current = key
        .to_cow_str()
        .parse::<ReadableKey>()
        .ok()
        .and_then(|k| config.read_string(&k).ok());
    if let Some(current) = current {
        if current == uri {
            eprintln!("The `{key}` configuration setting already points to this key.");
            return Ok(());
        }
        if cryptoki.get_public_key_pem(Some(&current)).is_ok() {
            eprintln!("The `{key}` configuration setting was left unchanged, as it points to another existing key:");
            eprintln!("  currently selected key: {current}");
            eprintln!("  newly created key:      {uri}");
            eprintln!("To select the new key, run:");
            eprintln!("  tedge config set {key} \"{uri}\"");
            return Ok(());
        }
        eprintln!("Note: `{key}` pointed to a key that no longer exists on the token: {current}");
    }

    config
        .update_toml(&|dto, _reader| {
            let r = dto.try_update_str(&key, uri).map_err(|e| e.into());
            eprintln!(
                "The `{key}` configuration setting was updated with the newly created key's URI"
            );
            r
        })
        .await
        .map_err(anyhow::Error::new)?;

    Ok(())
}

/// Given a cloud (and possibly profile) return the correct `device.key_uri` key under the correct table.
///
/// - `device.key_uri` if cloud is `None`
/// - `{cloud_name}.device.key_uri` if cloud is `Some(Cloud{profile: None})`
/// - `{cloud_name}.profiles.{profile}.device.key_uri` if cloud is `Some(Cloud{profile: Some("profile")})`
fn extract_device_id_for_cloud(
    cloud: Option<&crate::cli::common::MaybeBorrowedCloud<'static>>,
) -> anyhow::Result<tedge_config::tedge_toml::WritableKey> {
    // XXX: can break if the keys ever change and having to use strings sucks

    // Additionally, these kind of config transformation should probably live somewhere in tedge-config, but it's
    // currently (#3835) being rewritten to support generic mapper configurations which will make using per-profile
    // settings (which device.key_uri is) much simpler and so the cleaner implementation of this would have to be
    // updated anyway. As such, it's left as is and when generic mapping configurations are merged, it will have to be
    // revised.
    let mut key = "device.key_uri".to_string();
    let profile = cloud.as_ref().and_then(|c| c.profile_name());
    if let Some(profile) = profile {
        key = format!("profiles.{profile}.{key}");
    }

    let cloud = cloud.map(|c| match c {
        crate::cli::common::MaybeBorrowedCloud::Aws(_) => "aws",
        crate::cli::common::MaybeBorrowedCloud::Azure(_) => "az",
        crate::cli::common::MaybeBorrowedCloud::C8y(_) => "c8y",
        crate::cli::common::MaybeBorrowedCloud::Custom(_) => {
            unreachable!("certificate key_uri is not applicable for custom mappers")
        }
    });

    if let Some(cloud) = cloud {
        key = format!("{cloud}.{key}");
    }

    key.parse::<WritableKey>()
        .with_context(|| format!("failed to parse '{key}' as a WritableKey"))
}

/// Builds a PKCS #11 URI that selects a key on `token_uri` by its label (and id, if provided).
///
/// Used to check whether a matching key already exists on the token so it can be reused.
fn existing_key_uri(token_uri: &str, label: &str, id: Option<&[u8]>) -> String {
    let mut uri = format!("{token_uri};object={}", encode_uri_attr(label));
    if let Some(id) = id {
        uri.push_str(";id=");
        for byte in id {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Percent-encodes a PKCS #11 URI attribute value (RFC 7512), encoding any character that is not
/// unreserved so the value survives round-tripping through the URI parser.
pub(crate) fn encode_uri_attr(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Parses id provided as a sequence of bytes encoded as pair of hex digits without `0x` prefix.
/// Whitespace or colon separators between bytes (as printed by e.g. p11tool) are ignored.
pub(crate) fn parse_id(id_hexstr: &str) -> anyhow::Result<Vec<u8>> {
    let id_hexstr = id_hexstr.trim();

    anyhow::ensure!(!id_hexstr.is_empty(), "ID must not be empty");

    let mut bytes = Vec::new();
    let mut chars = id_hexstr.char_indices();
    while let Some((i1, c1)) = chars.next() {
        if c1.is_whitespace() || c1 == ':' {
            continue;
        }

        anyhow::ensure!(c1.is_ascii_hexdigit(), "{c1} is not a hex digit");

        let Some((i2, c2)) = chars.next() else {
            anyhow::bail!("expected hex digit after {c1}");
        };
        anyhow::ensure!(c2.is_ascii_hexdigit(), "expected hex digit after {c1}");

        let num = u8::from_str_radix(&id_hexstr[i1..=i2], 16)
            .with_context(|| format!("failed to parse {} as u8", &id_hexstr[i1..=i2]))?;
        bytes.push(num);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::cli::common::MaybeBorrowedCloud;

    use super::*;

    /// A [`TedgeP11Service`] stub where only [`get_public_key_pem`] works: it succeeds for the
    /// URIs listed in `existing_keys` and fails for anything else.
    ///
    /// [`TedgeP11Service`]: tedge_p11::service::TedgeP11Service
    /// [`get_public_key_pem`]: tedge_p11::service::TedgeP11Service::get_public_key_pem
    struct FakeCryptoki {
        existing_keys: Vec<&'static str>,
    }

    impl tedge_p11::service::TedgeP11Service for FakeCryptoki {
        fn get_public_key_pem(&self, uri: Option<&str>) -> anyhow::Result<String> {
            match uri {
                Some(uri) if self.existing_keys.contains(&uri) => Ok("PEM".to_string()),
                _ => anyhow::bail!("no key found"),
            }
        }

        fn choose_scheme(
            &self,
            _: tedge_p11::service::ChooseSchemeRequest,
        ) -> anyhow::Result<tedge_p11::service::ChooseSchemeResponse> {
            unimplemented!()
        }

        fn sign(
            &self,
            _: tedge_p11::service::SignRequestWithSigScheme,
        ) -> anyhow::Result<tedge_p11::service::SignResponse> {
            unimplemented!()
        }

        fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
            unimplemented!()
        }

        fn list_tokens(&self) -> anyhow::Result<tedge_p11::service::ListTokensResponse> {
            unimplemented!()
        }

        fn list_keys(
            &self,
            _: tedge_p11::service::ListKeysRequest,
        ) -> anyhow::Result<tedge_p11::service::ListKeysResponse> {
            unimplemented!()
        }

        fn change_pin(
            &self,
            _: tedge_p11::service::ChangePinRequest,
        ) -> anyhow::Result<tedge_p11::service::ChangePinResponse> {
            unimplemented!()
        }

        fn delete_key(
            &self,
            _: tedge_p11::service::DeleteKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::DeleteKeyResponse> {
            unimplemented!()
        }

        fn create_key(
            &self,
            _: CreateKeyRequest,
        ) -> anyhow::Result<tedge_p11::service::CreateKeyResponse> {
            unimplemented!()
        }

        fn init_token(
            &self,
            _: InitTokenRequest,
        ) -> anyhow::Result<tedge_p11::service::InitTokenResponse> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn does_not_overwrite_key_uri_pointing_to_another_existing_key() {
        let tempdir = TempTedgeDir::new();
        tempdir
            .file("tedge.toml")
            .with_raw_content("[device]\nkey_uri = \"pkcs11:token=tedge;object=old-key\"\n");
        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        let cryptoki = FakeCryptoki {
            existing_keys: vec!["pkcs11:token=tedge;object=old-key"],
        };

        save_key_uri_to_config(config, None, "pkcs11:token=tedge;object=new-key", &cryptoki)
            .await
            .unwrap();

        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        assert_eq!(
            config
                .read_string(&"device.key_uri".parse().unwrap())
                .unwrap(),
            "pkcs11:token=tedge;object=old-key"
        );
    }

    #[tokio::test]
    async fn overwrites_key_uri_pointing_to_a_key_that_no_longer_exists() {
        let tempdir = TempTedgeDir::new();
        tempdir
            .file("tedge.toml")
            .with_raw_content("[device]\nkey_uri = \"pkcs11:token=tedge;object=deleted-key\"\n");
        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        let cryptoki = FakeCryptoki {
            existing_keys: vec![],
        };

        save_key_uri_to_config(config, None, "pkcs11:token=tedge;object=new-key", &cryptoki)
            .await
            .unwrap();

        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        assert_eq!(
            config
                .read_string(&"device.key_uri".parse().unwrap())
                .unwrap(),
            "pkcs11:token=tedge;object=new-key"
        );
    }

    #[tokio::test]
    async fn keeps_key_uri_already_pointing_to_the_key() {
        let tempdir = TempTedgeDir::new();
        tempdir
            .file("tedge.toml")
            .with_raw_content("[device]\nkey_uri = \"pkcs11:token=tedge;object=my-key\"\n");
        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        let cryptoki = FakeCryptoki {
            existing_keys: vec!["pkcs11:token=tedge;object=my-key"],
        };

        save_key_uri_to_config(config, None, "pkcs11:token=tedge;object=my-key", &cryptoki)
            .await
            .unwrap();

        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        assert_eq!(
            config
                .read_string(&"device.key_uri".parse().unwrap())
                .unwrap(),
            "pkcs11:token=tedge;object=my-key"
        );
    }

    #[tokio::test]
    async fn saves_uri_under_correct_key() {
        let tempdir = TempTedgeDir::new();

        assert_saves_under_key(None, "device.key_uri", &tempdir).await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::c8y(None)),
            "c8y.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::c8y(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "c8y.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::az(None)),
            "az.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::az(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "az.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::aws(None)),
            "aws.device.key_uri",
            &tempdir,
        )
        .await;

        assert_saves_under_key(
            Some(&MaybeBorrowedCloud::aws(Some(
                "profile1".to_string().try_into().unwrap(),
            ))),
            "aws.profiles.profile1.device.key_uri",
            &tempdir,
        )
        .await;
    }

    async fn assert_saves_under_key(cloud: Option<&Cloud>, key: &str, tempdir: &TempTedgeDir) {
        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        let cryptoki = FakeCryptoki {
            existing_keys: vec![],
        };
        save_key_uri_to_config(config, cloud, "pkcs11:hello", &cryptoki)
            .await
            .unwrap();

        let config = TEdgeConfig::load(tempdir.path()).await.unwrap();
        assert_eq!(
            config.read_string(&key.parse().unwrap()).unwrap(),
            "pkcs11:hello"
        );

        std::fs::remove_file(tempdir.file("tedge.toml").path()).unwrap();
    }

    #[test]
    fn builds_existing_key_uri_by_label() {
        assert_eq!(
            existing_key_uri("pkcs11:token=tedge", "my-key", None),
            "pkcs11:token=tedge;object=my-key"
        );
    }

    #[test]
    fn builds_existing_key_uri_with_id() {
        assert_eq!(
            existing_key_uri("pkcs11:token=tedge", "my-key", Some(&[0x01, 0x02, 0xab])),
            "pkcs11:token=tedge;object=my-key;id=%01%02%AB"
        );
    }

    #[test]
    fn encodes_uri_attr_reserved_characters() {
        assert_eq!(encode_uri_attr("tedge"), "tedge");
        assert_eq!(encode_uri_attr("a b;c=d"), "a%20b%3Bc%3Dd");
    }

    #[test]
    fn parses_id() {
        assert_eq!(parse_id("01 02 03").unwrap(), vec![0x01, 0x02, 0x03]);
        assert_eq!(parse_id("010203").unwrap(), vec![0x01, 0x02, 0x03]);
        assert_eq!(parse_id("0102 03").unwrap(), vec![0x01, 0x02, 0x03]);
        // colon-separated, as printed by p11tool
        assert_eq!(parse_id("01:02:03").unwrap(), vec![0x01, 0x02, 0x03]);

        assert_eq!(
            parse_id("    ").unwrap_err().to_string(),
            "ID must not be empty"
        );

        assert_eq!(
            parse_id("0").unwrap_err().to_string(),
            "expected hex digit after 0"
        );

        assert_eq!(
            parse_id("  0   1   2   3 ").unwrap_err().to_string(),
            "expected hex digit after 0"
        );
    }
}
