//!
//! Tks specific backend using the AES/GCM item secrets encryption
//!
use crate::storage::collection::Collection;
use crate::storage::{StorageBackend, StorageBackendType, STORAGE, SecretsHandler};
use crate::tks_dbus::prompt_impl::{PromptAction, PromptDialog};
use crate::tks_error::TksError;
use dbus::arg::RefArg;
use fs::read;
use std::cell::RefCell;
use log::{debug, trace};
use openssl::hash::MessageDigest;
use openssl::pkcs5::pbkdf2_hmac;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::fs::DirBuilder;
use std::path::PathBuf;
use std::rc::Rc;
use uuid::Uuid;
use StorageBackendType::TksGcm;

pub struct TksGcmBackend {
    path: OsString,
    metadata_path: OsString,
    items_path: OsString,
    secrets_handler: SecretPasswordHandler,
}

struct SecretPasswordHandler {
    salt: Vec<u8>,
    key: Vec<u8>,
}
impl TksGcmBackend {
    pub(crate) fn new(path: OsString) -> Result<TksGcmBackend, TksError> {
        trace!("Initializing TksGcmBackend with {:?}", path);
        let mut metadata_path = PathBuf::new();
        metadata_path.push(path.clone());
        metadata_path.push("metadata");
        let _ = DirBuilder::new()
            .recursive(true)
            .create(metadata_path.clone())?;

        let mut items_path = PathBuf::new();
        items_path.push(path.clone());
        items_path.push("items");
        let _ = DirBuilder::new()
            .recursive(true)
            .create(items_path.clone())?;

        let mut salt_file_path = metadata_path.clone();
        salt_file_path.push("salt");
        let salt_check = fs::try_exists(salt_file_path.clone())?;
        let salt = if !salt_check {
            trace!("Initializing salt file {:?}", salt_file_path);
            // upon the very first initialization, generate a random salt
            let mut salt = vec![0u8; 256];
            openssl::rand::rand_bytes(&mut salt)?;
            fs::write(salt_file_path, salt.clone())?;
            salt.clone()
        } else {
            read(salt_file_path.clone())?
        };

        let backend = TksGcmBackend {
            path,
            metadata_path: metadata_path.into(),
            items_path: items_path.into(),
            secrets_handler: SecretPasswordHandler{
                salt,
                key: Vec::new(),
            },
        };
        Ok(backend)
    }

}

impl StorageBackend for TksGcmBackend {
    fn get_kind(&self) -> StorageBackendType {
        TksGcm
    }

    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError> {
        Ok(std::fs::read_dir(self.metadata_path.clone())?
            .into_iter()
            .filter(|e| e.is_ok())
            .map(|p| p.unwrap().path())
            .filter(|p| p.is_file())
            .collect())
    }

    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError> {
        let mut collection_path = PathBuf::new();
        collection_path.push(self.metadata_path.clone());
        collection_path.push(name);
        let mut items_path = PathBuf::new();
        items_path.push(self.items_path.clone());
        items_path.push(name);
        Ok((collection_path, items_path))
    }

    fn get_secrets_handler(&mut self) -> Result<Box<dyn SecretsHandler + '_>, TksError> {
        Ok(Box::new(&mut self.secrets_handler))
    }

    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        if !items_path.starts_with(self.items_path.clone()) {
            return Err(TksError::InternalError(
                "Items path not within the correct directory",
            ));
        }
        Ok("".to_string())
    }

    fn create_unlock_action(&mut self, coll_uuid: &Uuid) -> Result<PromptAction, TksError> {
        trace!("create_onlock_prompt for {:?}", coll_uuid);
        Ok(PromptAction {
            coll_uuid: coll_uuid.clone(),
            dialog: PromptDialog::PassphraseInput("Description", "Prompt", |s, coll_uuid| {
                let mut storage = STORAGE.lock()?;
                {
                    let mut secrets_handler = storage.backend.get_secrets_handler()?;
                    secrets_handler.derive_key_from_password(s)?;
                }
                let r = storage.modify_collection(coll_uuid, |c| {
                    let cypher = openssl::symm::Cipher::aes_256_gcm();
                    let key: Vec<u8> = Vec::new();
                    let iv: Vec<u8> = Vec::new();
                    let aad: Vec<u8> = Vec::new();
                    let tag: Vec<u8> = vec![0u8; 16];
                    let file_contents = read(c.items_path.clone())?;
                    let data = openssl::symm::decrypt_aead(
                        cypher,
                        &key,
                        Some(&iv),
                        &aad,
                        &file_contents,
                        &tag,
                    )?;
                    Ok(true)
                })?;
                Ok(r)
            }),
        })
    }

    fn save_collection_metadata(
        &mut self,
        collection: &mut Collection,
        x: &String,
        is_new: bool,
    ) -> Result<(), TksError> {
        todo!()
    }

    fn save_collection_items(
        &mut self,
        collection: &mut Collection,
        x: &String,
        x0: &String,
    ) -> Result<(), TksError> {
        todo!()
    }
}

impl SecretsHandler for &mut SecretPasswordHandler {
    fn derive_key_from_password(&mut self, s: SecretString) -> Result<(), TksError> {
        pbkdf2_hmac(
            s.expose_secret().as_bytes(),
            &self.salt,
            1024,
            MessageDigest::sha512(),
            &mut self.key,
        )?;
        // TODO derive real key from above key using KDF and current timestamp
        Ok(())
    }
}