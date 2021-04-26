use super::error::CertError;
use crate::system_command::Role;
use crate::utils::paths::ok_if_not_found;
use std::path::Path;

/*
use std::{
    fs::{File, OpenOptions},
    io::prelude::*,
    path::Path,
};
*/

pub trait FileInstaller {
    fn install(&self, path: &Path, role: Role, mode: u32, data: &[u8]) -> Result<(), CertError>;
    fn remove_if_exists(&self, path: &Path) -> Result<(), CertError>;
}

pub struct Installer;

impl FileInstaller for Installer {
    fn install(&self, path: &Path, role: Role, mode: u32, data: &[u8]) -> Result<(), CertError> {
        unimplemented!()
        //cert_file.write_all(cert_pem.as_bytes())?;
        //cert_file.sync_all()?;
        // Prevent the certificate to be overwritten
        // paths::set_permission(&cert_file, 0o444)?;
        //fn create_new_file(path: impl AsRef<Path>) -> Result<File, CertError> {
        //   Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
        //}
    }

    fn remove_if_exists(&self, path: &Path) -> Result<(), CertError> {
        let () = std::fs::remove_file(path).or_else(ok_if_not_found)?;
        Ok(())
    }
}
