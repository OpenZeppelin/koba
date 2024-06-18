use alloy::signers::local::{LocalSigner, PrivateKeySigner};
use eyre::{eyre, Context, Result};
use std::fs;

use crate::config::PrivateKey;

impl PrivateKey {
    pub fn wallet(&self) -> Result<PrivateKeySigner> {
        if let Some(key) = &self.private_key {
            return Ok(key.parse::<PrivateKeySigner>()?);
        }

        if let Some(file) = &self.private_key_path {
            let key = fs::read_to_string(file).wrap_err("could not open private key file")?;
            return Ok(key.parse::<PrivateKeySigner>()?);
        }

        let keystore = self
            .keystore_path
            .as_ref()
            .ok_or(eyre!("no keystore file"))?;
        let password = self
            .keystore_password_path
            .as_ref()
            .map(fs::read_to_string)
            .unwrap_or(Ok("".into()))?;

        LocalSigner::decrypt_keystore(keystore, password).wrap_err("could not decrypt keystore")
    }
}
