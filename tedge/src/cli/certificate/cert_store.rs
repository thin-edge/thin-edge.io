use super::error::CertError;
use crate::utils::paths::ok_if_not_found;
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::OpenOptions, io::prelude::*, path::Path};

/// A certificate store can store public and private keys and remove them again. It takes care of
/// ensuring that the backing files have the right permissions to avoid leaking private keys to
/// other system users.
///
/// Having this separated from the rest of the certificate creation code allows us to test storing
/// the certificate and private key under various conditions (with or w/o sudo etc.) and validate
/// it's correct and secure behavior!
pub trait CertificateStore {
    fn store_certificate(&self, path: &Path, data: &[u8]) -> Result<(), CertError>;
    fn store_private_key(&self, path: &Path, data: &[u8]) -> Result<(), CertError>;
    fn remove_certificate(&self, path: &Path) -> Result<(), CertError>;
    fn remove_private_key(&self, path: &Path) -> Result<(), CertError>;
}

/// Certificate store implementation that stores certificates and keys to be later used by the
/// broker (mosquitto) user.
pub struct BrokerCertStore;

const BROKER_USER: &str = "mosquitto";

impl CertificateStore for BrokerCertStore {
    fn store_certificate(&self, path: &Path, data: &[u8]) -> Result<(), CertError> {
        // Store the certificate with permissions 0o444 (u-r, g-r, o-r)
        // to prevent the certificate from being overwritten.

        // XXX: Instead of this, you can also `chown` the file after writing it!
        let user = users::get_user_by_name(BROKER_USER).unwrap(); // XXX
        let group = users::get_group_by_name(BROKER_USER).unwrap(); // XXX
        let guard = users::switch::switch_user_group(user.uid(), group.gid()).unwrap();

        let mut file = OpenOptions::new()
            .mode(0o444)
            .write(true)
            .create_new(true)
            .open(path)?;

        let () = file.write_all(data)?;
        let () = file.sync_all()?;

        drop(guard);

        Ok(())
    }

    fn store_private_key(&self, path: &Path, data: &[u8]) -> Result<(), CertError> {
        // XXX: Instead of this, you can also `chown` the file after writing it!
        let user = users::get_user_by_name(BROKER_USER).unwrap(); // XXX
        let group = users::get_group_by_name(BROKER_USER).unwrap(); // XXX
        let guard = users::switch::switch_user_group(user.uid(), group.gid()).unwrap();

        let mut file = OpenOptions::new()
            .mode(0o400)
            .write(true)
            .create_new(true)
            .open(path)?;

        let () = file.write_all(data)?;
        let () = file.sync_all()?;

        drop(guard);

        Ok(())
    }

    fn remove_certificate(&self, path: &Path) -> Result<(), CertError> {
        let () = std::fs::remove_file(path).or_else(ok_if_not_found)?;
        Ok(())
    }

    fn remove_private_key(&self, path: &Path) -> Result<(), CertError> {
        let () = std::fs::remove_file(path).or_else(ok_if_not_found)?;
        Ok(())
    }
}
