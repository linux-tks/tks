//!
//! Tks specific backend using the AES/GCM item secrets encryption
//!
use crate::storage::collection::Collection;
use crate::storage::tks_gcm::TksGcmPasswordSecretHandlerState::{KeyAvailable, Locked, NotCommissioned};
use crate::storage::{SecretsHandler, StorageBackend, StorageBackendType, STORAGE};
use crate::tks_dbus::prompt_impl::{PromptAction, PromptDialog};
use crate::tks_error::TksError;
use dbus::arg::RefArg;
use log::{debug, trace};
use openssl::rand::rand_bytes;
use openssl::symm::decrypt_aead;
use secrecy::{ExposeSecret, SecretString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::fs::{try_exists, DirBuilder, File};
use std::path::{PathBuf, MAIN_SEPARATOR};
use std::rc::Rc;
use uuid::Uuid;
use StorageBackendType::TksGcm;

pub struct TksGcmBackend {
    path: OsString,
    metadata_path: OsString,
    items_path: OsString,
    secrets_handler: TksGcmPasswordSecretHandler,
}

enum TksGcmPasswordSecretHandlerState {
    /// Backend has this state when TKS is freshly installed or reconfigured
    NotCommissioned,
    /// Backend already had a password defined by the user but a password prompt is yet to be produced
    Locked,
    /// A successful password prompt already occurred
    KeyAvailable,
}

struct TksGcmPasswordSecretHandler {
    state: TksGcmPasswordSecretHandlerState,
    salt: Vec<u8>,
    commissioned_data: Vec<u8>,
    commissioned_data_path: OsString,
    key: Vec<u8>,
    cipher: openssl::symm::Cipher,
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

        let mut salt_file_path = path.clone();
        salt_file_path.push(std::path::MAIN_SEPARATOR_STR);
        salt_file_path.push("salt");
        let salt_check = fs::try_exists(salt_file_path.clone())?;
        let secret_state: TksGcmPasswordSecretHandlerState;
        let salt = if !salt_check {
            trace!("Initializing salt file {:?}", salt_file_path);
            // upon the very first initialization, generate a random salt
            let mut salt = vec![0u8; 256];
            openssl::rand::rand_bytes(&mut salt)?;
            fs::write(salt_file_path, salt.clone())?;
            secret_state = TksGcmPasswordSecretHandlerState::NotCommissioned;
            salt.clone()
        } else {
            secret_state = TksGcmPasswordSecretHandlerState::Locked;
            fs::read(salt_file_path.clone())?
        };

        let mut commissioned_data_path = path.clone();
        commissioned_data_path.push(std::path::MAIN_SEPARATOR_STR);
        commissioned_data_path.push("commissioned");
        let mut commissioned_data: Vec<u8> = vec![0u8; 256];
        match secret_state {
            TksGcmPasswordSecretHandlerState::NotCommissioned => {
                commissioned_data = vec![0u8; 256];
                rand_bytes(&mut commissioned_data)?;
            }
            TksGcmPasswordSecretHandlerState::Locked => {
                // commissioned data will be checked upon next password prompt
            }
            TksGcmPasswordSecretHandlerState::KeyAvailable => {
                unreachable!()
            }
        }

        let backend = TksGcmBackend {
            path,
            metadata_path: metadata_path.into(),
            items_path: items_path.into(),
            secrets_handler: TksGcmPasswordSecretHandler {
                state: secret_state,
                salt,
                commissioned_data,
                commissioned_data_path,
                key: vec![0u8; 32],
                cipher: openssl::symm::Cipher::aes_256_gcm(),
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

    fn collection_items_path(&self, name: &str) -> Result<PathBuf, TksError> {
        let mut items_path = PathBuf::new();
        items_path.push(self.items_path.clone());
        items_path.push(name);
        Ok(items_path)
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

    /// this actually would unlock the secrets_handler, as all the collections on this backend
    /// type share the same password
    fn create_unlock_action(
        &mut self,
        coll_uuid: &Uuid,
        coll_name: &str,
    ) -> Result<PromptAction, TksError> {
        trace!("create_onlock_action for {:?}", coll_uuid);
        let description = if matches!(
            &self.secrets_handler.state,
            TksGcmPasswordSecretHandlerState::NotCommissioned
        ) {
            format!(
                "Define the TKS unlock password, so we can store the new collection '{}'",
                coll_name
            )
        } else {
            format!(
                "Enter the TKS unlock password, so we can unlock the collection '{}'",
                coll_name
            )
        };
        let confirmation = if matches!(
            &self.secrets_handler.state,
            TksGcmPasswordSecretHandlerState::NotCommissioned
        ) {
            Some("Confirm password".to_string())
        } else {
            None
        };
        let mismatch = if matches!(
            &self.secrets_handler.state,
            TksGcmPasswordSecretHandlerState::NotCommissioned
        ) {
            Some("Passwords do not match".to_string())
        } else {
            None
        };
        Ok(PromptAction {
            coll_uuid: coll_uuid.clone(),
            dialog: PromptDialog::PassphraseInput(
                description,
                "Password".to_string(),
                confirmation,
                mismatch,
                |s, coll_uuid| {
                    trace!("create_unlock_action: Performing unlock action");
                    let mut storage = STORAGE.lock()?;
                    {
                        let mut secrets_handler = storage.backend.get_secrets_handler()?;
                        secrets_handler.derive_key_from_password(s)?;
                    }
                    storage.unlock_collection(coll_uuid)?;
                    Ok(true)
                },
            ),
        })
    }

    fn save_collection_metadata(
        &mut self,
        collection: &mut Collection,
        x: &String,
    ) -> Result<(), TksError> {
        trace!("save_collection_metadata {:?}", &collection.path);
        fs::write(&collection.path, x)?;
        Ok(())
    }

    fn save_collection_items(
        &mut self,
        collection: &mut Collection,
        metadata: &String,
        items: &String,
    ) -> Result<(), TksError> {
        trace!("save_collection_items {:?}", &collection.items_path);
        let secrets_handler = &self.secrets_handler;
        let items_encrypted = secrets_handler.encrypt_aead(metadata, items.as_ref())?;
        fs::write(&collection.items_path, items_encrypted)?;
        Ok(())
    }

    /// NOTE: this returns an empty vector if no items file is present
    fn load_collection_items(
        &self,
        collection: &Collection,
        metadata: &String,
    ) -> Result<Vec<u8>, TksError> {
        trace!("load_collection_items {:?}", &collection.items_path);

        let mut encrypted: Vec<u8> = Vec::new();
        if try_exists(&collection.items_path)? {
            encrypted = fs::read(&collection.items_path)?;
            self.secrets_handler.decrypt_aead(metadata, &encrypted)
        } else {
            Ok(encrypted)
        }
    }
}

impl SecretsHandler for &mut TksGcmPasswordSecretHandler {
    fn derive_key_from_password(&mut self, s: SecretString) -> Result<(), TksError> {
        trace!("derive_key_from_password");
        let mut key= vec![0u8; 32];
        openssl::pkcs5::pbkdf2_hmac(
            s.expose_secret().as_bytes(),
            &self.salt,
            1024,
            openssl::hash::MessageDigest::sha512(),
            &mut key,
        )?;

        match self.state {
            NotCommissioned => {
                trace!("Commissioning the storage backend");
                let metadata = self.commissioned_data_path.to_str().unwrap();
                let encrypted = self.encrypt_aead(metadata, &self.commissioned_data)?;
                fs::write(&self.commissioned_data_path, encrypted)?;
            },
            Locked => {
                trace!("Checking storage backend password");
                let data = fs::read(&self.commissioned_data_path)?;
                let metadata = self.commissioned_data_path.to_str().unwrap();
                let _ = self.decrypt_aead(metadata, &data)?;
            },
            KeyAvailable => {
                unreachable!()
            },
        }
        // TODO derive real key from above key using KDF and current timestamp
        self.state = KeyAvailable;
        Ok(())
    }
}

impl TksGcmPasswordSecretHandler {
    const FILE_SCHEMA_VERSION: u8 = 1;
    fn encrypt_aead(&self, metadata: &str, items: &[u8]) -> Result<Vec<u8>, TksError> {
        let mut tag = vec![0u8; 16];
        let mut iv = [0u8; 12];
        rand_bytes(&mut iv)?;
        let ciphertext = openssl::symm::encrypt_aead(
            self.cipher,
            &self.key,
            Some(&iv),
            metadata.as_ref(),
            items.as_ref(),
            &mut tag,
        )?;
        // here we build the structure of the items file
        let mut encrypted: Vec<u8> = Vec::new();
        encrypted.push(Self::FILE_SCHEMA_VERSION);
        encrypted.extend_from_slice(&iv);
        encrypted.extend_from_slice(&tag);
        encrypted.extend_from_slice(&ciphertext);
        Ok(encrypted)
    }

    fn decrypt_aead(&self, metadata: &str, encrypted: &[u8]) -> Result<Vec<u8>, TksError> {
        let version: &u8 = encrypted
            .get(0)
            .ok_or_else(|| TksError::SerializationError("Corrupted file".to_string()))?;
        let mut tag: Vec<u8> = vec![0u8; 16];
        let mut iv: Vec<u8> = vec![0u8; 12];
        let mut cyphertext: Vec<u8> = Vec::new();
        match version {
            1 => {
                iv = encrypted
                    .get(1..13)
                    .ok_or_else(|| TksError::SerializationError("Corrupted file".to_string()))?
                    .into();
                tag = encrypted
                    .get(13..29)
                    .ok_or_else(|| TksError::SerializationError("Corrupted file".to_string()))?
                    .into();
                cyphertext = encrypted
                    .get(29..)
                    .ok_or_else(|| TksError::SerializationError("Corrupted file".to_string()))?
                    .into();
            }
            _ => {
                return Err(TksError::SerializationError(
                    "Unknown file version".to_string(),
                ))
            }
        };

        let decrypted = decrypt_aead(
            self.cipher,
            &self.key,
            Some(&iv),
            metadata.as_ref(),
            cyphertext.as_ref(),
            tag.as_ref(),
        )?;
        Ok(decrypted)
    }
}
