use lazy_static::lazy_static;
use log::{error, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;
use dbus::arg::{RefArg};
use secrecy::SecretString;
use uuid::Uuid;
use collection::{Collection, Item, ItemData};
#[cfg(feature = "fscrypt")]
use fscrypt::FSCryptBackend;

use crate::settings::SETTINGS;
use crate::storage::tks_gcm::TksGcmBackend;
use crate::tks_dbus::prompt_impl::{PromptAction};
use crate::tks_error::TksError;

pub(crate) mod collection;
#[cfg(feature = "fscrypt")]
mod fscrypt;
mod tks_gcm;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CollectionSecrets {
    items: Vec<ItemData>,
}

static DEFAULT_NAME: &'static str = "default";

pub struct Storage {
    backend: Box<dyn StorageBackend + Send>,
    pub collections: Vec<Collection>,
}

lazy_static! {
    pub static ref STORAGE: Arc<Mutex<Storage>> = Arc::new(Mutex::new(Storage::new()));
}

enum StorageBackendType {
    /// Use EXPERIMENTAL fscrypt to handle item encryption on disk
    /// https://github.com/google/fscrypt
    /// Backend should have been previously commissioned
    FSCrypt,
    TksGcm,
}

trait SecretsHandler {
  fn derive_key_from_password(&mut self, s: SecretString) -> Result<(), TksError>;
}
trait StorageBackend {
    fn get_kind(&self) -> StorageBackendType;
    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError>;
    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError>;
    fn collection_items_path(&self, name: &str) -> Result<PathBuf, TksError>;
    fn get_secrets_handler(&mut self) -> Result<Box<dyn SecretsHandler + '_>, TksError>;
    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError>;
    fn create_unlock_action(&mut self, coll_uuid: &Uuid, coll_name: &str) -> Result<PromptAction, TksError>;
    fn save_collection_metadata(&mut self, collection: &mut Collection, x: &String) -> Result<(), TksError>;
    fn save_collection_items(&mut self, collection: &mut Collection, x: &String, x0: &String) -> Result<(), TksError>;
    fn load_collection_items(&self, collection: &Collection, metadata: &String) -> Result<Vec<u8>, TksError>;
}

impl Storage {
    fn new() -> Self {
        let do_create_storage = || {
            let settings = SETTINGS.lock().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting settings: {}", e),
                )
            })?;
            let backend = Box::new(match settings.storage.kind.as_str() {
                // #[cfg(feature = "fscrypt")]
                // "fscrypt" => FSCryptBackend::new(OsString::from(settings.storage.path.clone()))?,
                "tks_gcm" => TksGcmBackend::new(OsString::from(settings.storage.path.clone()))?,
                _ => panic!("Unknown storage backend kind specified in the configuration file"),
            });
            let collections = backend
                .as_ref()
                .get_metadata_paths()?
                .into_iter()
                .map(|p| Self::load_collection(&p))
                .collect::<Result<Vec<_>, _>>()?;
            let mut storage = Storage {
                backend,
                collections,
            };
            for c in storage.collections.iter_mut() {
                c.items_path = storage.backend.collection_items_path(&c.name)?;
            }

            // look for the default collection and create it if it doesn't exist
            let _ = storage.read_alias("default").or_else(|_| {
                info!("Creating default collection");
                storage
                    .create_collection(DEFAULT_NAME, DEFAULT_NAME, &HashMap::new())
                    .map(|_| "default".to_string())
            })?;

            Ok(storage)
        };

        do_create_storage().unwrap_or_else(|e: TksError| {
            panic!("Error initializing storage: {:}", e);
        })
    }

    pub fn read_alias(&mut self, alias: &str) -> Result<String, TksError> {
        self.collections
            .iter()
            .filter(|c| c.aliases.is_some())
            .find(|&c| c.aliases.as_ref().unwrap().contains(&alias.to_string()))
            .map(|c| c.name.clone())
            .ok_or(TksError::NotFound(
                format!("Alias '{}' not found", alias).into(),
            ))
    }

    pub fn with_collection<F, T>(&self, uuid: &Uuid, f: F) -> Result<T, TksError>
    where
        F: FnOnce(&Collection) -> Result<T, TksError>,
    {
        let mut collection = self
            .collections
            .iter()
            .find(|c| c.uuid == *uuid)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Collection '{}' not found", uuid),
                )
            })?;
        f(&mut collection)
    }

    pub fn modify_collection<F, T>(&mut self, uuid: &Uuid, f: F) -> Result<T, TksError>
    where
        F: FnOnce(&mut Collection) -> Result<T, TksError>,
    {
        let result = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == *uuid)
            .ok_or(TksError::NotFound(
                format!("Collection '{}' not found", uuid).into(),
            ))
            .and_then(|c| f(c));

        // TODO the collection name may have changed; in this case, we might need to also
        // update the collection's path on disk; but for the moment, it should still reload
        // fine as the correct collection name gets serialized on disk
        self.save_collection(uuid, false)?;
        result
    }

    /// This performs a read-only operation on a collection item
    /// for RW operations, use modify_item
    pub fn with_item<F, T>(
        &mut self,
        collection_uuid: &Uuid,
        item_uuid: &Uuid,
        f: F,
    ) -> Result<T, TksError>
    where
        F: FnOnce(&Item) -> Result<T, TksError>,
    {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == *collection_uuid)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Collection '{}' not found", collection_uuid),
            ))?;
        let item = collection.get_item(item_uuid)?;
        f(item)
    }

    pub fn modify_item<F, T>(
        &mut self,
        collection_uuid: &Uuid,
        item_uuid: &Uuid,
        f: F,
    ) -> Result<T, TksError>
    where
        F: FnOnce(&mut Item) -> Result<T, TksError>,
    {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == *collection_uuid)
            .ok_or_else(|| {
                error!("Collection not found: {}", collection_uuid);
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Collection not found".to_string(),
                )
            })?;
        let mut item = collection.get_item_mut(item_uuid)?;
        match f(&mut item) {
            Ok(t) => {
                item.modified = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .into();
                self.save_collection(collection_uuid, false)?;
                Ok(t)
            }
            Err(e) => Err(e),
        }
    }

    /// Create a new collection
    ///
    /// # Arguments
    /// * `name` - The name of the collection
    /// * `properties` - A HashMap of properties to set on the collection; this version ignores
    /// these properties and this is allowed by the spec
    pub fn create_collection(
        &mut self,
        name: &str,
        alias: &str,
        _properties: &HashMap<String, String>,
    ) -> Result<Uuid, TksError> {
        let (path, items_path) = self.backend.new_metadata_path(name)?;
        let mut coll = Collection::new(name, &path, &items_path)?;
        if !alias.is_empty() {
            coll.aliases = Some(vec![alias.to_string()]);
        }
        let uuid = coll.uuid;
        self.collections.push(coll);
        self.save_collection(&uuid, true)?;
        trace!("Created collection '{}' at path '{:?}'", uuid, path);
        Ok(uuid)
    }

    fn save_collection(&mut self, uuid: &Uuid, is_new: bool) -> Result<(), TksError> {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))?;
        trace!(
            "Saving collection '{}' to path '{}'",
            collection.name,
            collection.path.display()
        );
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Error getting system time: {}", e),
                )
            })?
            .as_secs()
            .into();
        collection.modified = ts;

        let mut metadata = serde_json::to_string(&collection)?;
        self.backend.save_collection_metadata(collection, &metadata)?;

        if !collection.locked {
            // add file paths to the authentication metadata to reduce attack surface
            metadata.push_str(collection.path.to_str().unwrap());
            metadata.push_str(collection.items_path.to_str().unwrap());

            let collection_secrets = collection.get_secrets();
            let items = serde_json::to_string(&collection_secrets)?;
            self.backend.save_collection_items(collection, &metadata, &items)?;
        }
        Ok(())
    }

    /// Loads collection metadata from disk.
    /// The resulting collection is in a locked state.
    fn load_collection(path: &PathBuf) -> Result<Collection, TksError> {
        trace!("Loading collection from path '{}'", path.display());
        let mut file = File::open(path)?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let mut collection: Collection = serde_json::from_str(&data)?;
        collection.path = path.clone();
        collection.locked = true;
        collection
            .items
            .iter_mut()
            .for_each(|i: &mut Item| i.id.collection_uuid = collection.uuid);
        Ok(collection)
    }

    fn unlock_collection(&mut self, coll_uuid: &Uuid) -> Result<(), TksError> {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == *coll_uuid)
            .ok_or_else(|| TksError::NotFound(None))?;
        trace!(
            "unlock_collection '{}' from path '{}'",
            collection.name,
            collection.path.display()
        );

        // prepare the authentication metadata
        assert!(collection.items_path.to_str().unwrap().len() >0);
        let mut metadata = serde_json::to_string(&collection)?;
        metadata.push_str(collection.path.to_str().unwrap());
        metadata.push_str(collection.items_path.to_str().unwrap());

        // ask backend to decrypt the items, if any
        let decrypted_items = self.backend.load_collection_items(collection, &metadata)?;
        collection.unlock(&decrypted_items)?;
        Ok(())
    }

    pub(crate) fn create_unlock_action(&mut self, coll_uuid: &Uuid) -> Result<PromptAction, TksError> {
        let collection = self
            .collections
            .iter()
            .find(|c| c.uuid == *coll_uuid)
            .ok_or_else(|| TksError::NotFound(None))?;
        self.backend.create_unlock_action(coll_uuid, &collection.name)
    }
}
