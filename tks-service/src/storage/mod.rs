use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::DirBuilder;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;
use uuid::Uuid;
use collection::{Collection, Item, ItemData};

use crate::settings::SETTINGS;
use crate::tks_dbus::prompt_impl::{TksFscryptPrompt};
use crate::tks_error::TksError;

pub(crate) mod collection;

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
    /// Use fscrypt to handle item encryption on disk
    /// https://github.com/google/fscrypt
    /// Backend should have been previously commissioned
    FSCrypt,
}

trait StorageBackend {
    fn get_kind(&self) -> StorageBackendType;
    fn get_metadata_paths(&self) -> Result<Vec<PathBuf>, TksError>;
    fn new_metadata_path(&self, name: &str) -> Result<(PathBuf, PathBuf), TksError>;
    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError>;
    fn create_unlock_prompt(&self, coll_uuid: &Uuid) -> Result<dbus::Path<'static>, TksError>;
}

struct FSCryptBackend {
    path: OsString,
    metadata_path: OsString,
    items_path: OsString,
    commissioned: bool,
}

impl FSCryptBackend {
    fn new(path: OsString) -> Result<FSCryptBackend, TksError> {
        debug!("Initializing fscrypt storage at {:?}", path);
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

        let commissioned = false;
        let backend = FSCryptBackend {
            path,
            metadata_path: metadata_path.into(),
            items_path: items_path.into(),
            commissioned,
        };
        Ok(backend)
    }
}

impl StorageBackend for FSCryptBackend {
    fn get_kind(&self) -> StorageBackendType {
        StorageBackendType::FSCrypt
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

    fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        if !items_path.starts_with(self.items_path.clone()) {
            return Err(TksError::InternalError(
                "Items path not within the correct directory",
            ));
        }
        if !self.commissioned {
            return Err(TksError::BackendError(format!(
                "Storage in directory {:?} is not commissioned",
                self.items_path
            )));
        }
        Ok("".to_string())
    }

    fn create_unlock_prompt(&self, coll_uuid: &Uuid) -> Result<dbus::Path<'static>, TksError> {
        trace!("create_onlock_prompt for {:?}", coll_uuid);
        Ok(TksFscryptPrompt::new(coll_uuid))
    }
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
                "fscrypt" => FSCryptBackend::new(OsString::from(settings.storage.path.clone()))?,
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
    /// # Returns
    /// * `Ok(())` - The collection was created successfully
    /// * `Err(std::io::Error)` - There was an error creating the collection
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
        let mut file = if is_new {
            File::create_new(collection.path.clone())?
        } else {
            File::create(collection.path.clone())?
        };
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
        serde_json::to_writer_pretty(&mut file, collection)?;
        if !collection.locked {
            debug!("Collection items path: {}", collection.items_path.display());
            let mut file = File::create(collection.items_path.clone())?;
            let collection_secrets = collection.get_secrets();
            serde_json::to_writer_pretty(&mut file, &collection_secrets)?;
        }
        Ok(())
    }

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
    pub(crate) fn unlock_items(&self, items_path: &PathBuf) -> Result<String, TksError> {
        self.backend.unlock_items(items_path)
    }
    pub(crate) fn create_unlock_prompt(&self, coll_uuid: &Uuid) -> Result<dbus::Path<'static>, TksError> {
        self.backend.create_unlock_prompt(coll_uuid)
    }
}
