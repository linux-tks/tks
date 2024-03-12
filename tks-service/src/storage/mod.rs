use crate::tks_dbus::session_impl::Session;
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::DirBuilder;
use std::fs::File;
use std::io::Read;
use std::path::{PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::settings::SETTINGS;
use crate::tks_dbus::prompt_impl::{TksFscryptPrompt, TksPrompt};
use crate::tks_error::TksError;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ItemData {
    uuid: Uuid,
    data: Vec<u8>,
    pub content_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CollectionSecrets {
    items: Vec<ItemData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Item {
    pub label: String,
    pub created: u64,
    pub modified: u64,
    pub attributes: HashMap<String, String>,
    pub id: ItemId,

    // when Item is locked, this is None
    #[serde(skip)]
    pub data: Option<ItemData>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ItemId {
    pub uuid: Uuid,
    #[serde(skip)]
    pub collection_uuid: Uuid,
}

static DEFAULT_NAME: &'static str = "default";

#[derive(Serialize, Deserialize, Debug)]
pub struct Collection {
    schema_version: u8,
    pub uuid: Uuid,
    pub default: bool,
    pub name: String,
    pub items: Vec<Item>,
    pub aliases: Option<Vec<String>>,
    pub created: u64,
    pub modified: u64,

    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    items_path: PathBuf,
    #[serde(skip)]
    pub locked: bool,
    #[serde(skip)]
    pending_async: Option<JoinHandle<()>>,
}

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
    /// these properties and this is allowd by the spec
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

impl Collection {
    fn new(name: &str, path: &PathBuf, items_path: &PathBuf) -> Result<Collection, TksError> {
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
        let collection = Collection {
            uuid: Uuid::new_v4(),
            default: DEFAULT_NAME == name,
            schema_version: 1,
            name: name.to_string(),
            path: path.clone(),
            items_path: items_path.clone(),
            items: Vec::new(),
            aliases: None,
            locked: true,
            created: ts,
            modified: ts,
            pending_async: None,
        };

        Ok(collection)
    }

    pub fn create_item(
        &mut self,
        label: &str,
        properties: HashMap<String, String>,
        secret: (&Session, Vec<u8>, Vec<u8>, String),
        replace: bool,
        sender: String,
    ) -> Result<ItemId, TksError> {
        if self.locked {
            return Err(TksError::PermissionDenied);
        }
        let secret_session = secret.0;

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
        let uuid = Uuid::new_v4();
        let item = Item {
            label: label.to_string(),
            created: ts,
            modified: ts,
            data: Some(ItemData {
                uuid,
                data: match secret_session.decrypt(&secret.1, &secret.2, sender) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Cannot decrypt secret: {}", e);
                        return Err(TksError::CryptoError);
                    }
                },
                content_type: secret.3,
            }),
            id: ItemId {
                collection_uuid: self.uuid,
                uuid,
            },
            attributes: properties,
        };
        let item = if let Some(index) = self.items.iter().position(|i| {
            i.attributes == item.attributes
                && match (&i.data, &item.data) {
                    (Some(d1), Some(d2)) => {
                        d1.content_type == d2.content_type && d1.data == d2.data
                    }
                    (None, None) => true,
                    _ => false,
                }
        }) {
            if replace {
                self.items[index] = item;
                self.items.get(index).unwrap()
            } else {
                return Err(TksError::Duplicate);
            }
        } else {
            self.items.push(item);
            self.items.last().unwrap()
        };
        let item_id = item.id.clone();
        Ok(item_id)
    }

    pub fn get_item(&self, uuid: &Uuid) -> Result<&Item, TksError> {
        self.items
            .iter()
            .find(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
    }

    pub fn get_item_mut(&mut self, uuid: &Uuid) -> Result<&mut Item, TksError> {
        self.items
            .iter_mut()
            .find(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
    }

    pub fn delete_item(&mut self, uuid: &Uuid) -> Result<Item, TksError> {
        if self.locked {
            return Err(TksError::PermissionDenied);
        }
        self.items
            .iter()
            .position(|i| i.id.uuid == *uuid)
            .ok_or_else(|| TksError::NotFound(None))
            .and_then(|i| {
                let older = self.items.swap_remove(i);
                Ok(older)
            })
    }

    fn get_secrets(&self) -> CollectionSecrets {
        CollectionSecrets {
            items: self
                .items
                .iter()
                .map(|i| i.data.as_ref().unwrap().clone())
                .collect(),
        }
    }

    pub fn unlock(&mut self) -> Result<(), TksError> {
        if !self.locked || self.items.is_empty() {
            self.locked = false;
            return Ok(());
        }

        let collection_uuid = self.uuid.clone();
        let items_path = self.items_path.clone();

        self.pending_async = Some(tokio::spawn(async move {
            debug!("Performing collection unlock: {}", collection_uuid);
            let data = STORAGE
                .lock()
                .unwrap()
                .unlock_items(&items_path)
                .ok()
                .or(Some("".to_string()))
                .unwrap();
            let collection_secrets: CollectionSecrets = serde_json::from_str(&data)
                .ok()
                .or(Some(CollectionSecrets::new()))
                .unwrap();

            let _ = STORAGE
                .lock()
                .unwrap()
                .modify_collection(&collection_uuid, |c| {
                    c.items.iter_mut().for_each(|item| {
                        let _ = collection_secrets
                            .items
                            .iter()
                            .find(|s| s.uuid == item.id.uuid)
                            .ok_or(std::io::Error::new(
                                // TODO: maybe we should put the service in a fail state if we can't unlock a collection
                                std::io::ErrorKind::NotFound,
                                format!("Secrets file does not contain secret for item "),
                            ))
                            .and_then(|s| {
                                item.data = Some(s.clone());
                                Ok(())
                            });
                    });
                    c.locked = false;
                    Ok(())
                });
        }));
        Ok(())
    }
    pub fn lock(&mut self) -> Result<(), TksError> {
        self.locked = true;
        self.items.iter_mut().for_each(|item| item.data = None);
        Ok(())
    }
}

impl Item {
    pub fn get_secret(
        &self,
        session: &Session,
        sender: String,
    ) -> Result<(String, Vec<u8>, Vec<u8>, String), std::io::Error> {
        trace!("get_secret called on '{}'", self.label);
        let data = self.data.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("Item is locked"))
        })?;

        let (iv, secret) = session.encrypt(&data.data, sender).map_err(|e| {
            error!("Error encrypting secret: {}", e);
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Data cannot be prepared"),
            )
        })?;
        Ok(("".to_string(), iv, secret, data.content_type.clone()))
    }
    pub fn set_secret(
        &mut self,
        session: &Session,
        parameters: Vec<u8>,
        value: &Vec<u8>,
        content_type: String,
        sender: String,
    ) -> Result<(), TksError> {
        trace!("set_secret called on '{}'", self.label);
        self.data = Some(ItemData {
            uuid: self.id.uuid,
            data: session.decrypt(&parameters, value, sender)?,
            content_type,
        });
        Ok(())
    }
}

impl CollectionSecrets {
    pub fn new() -> CollectionSecrets {
        CollectionSecrets { items: Vec::new() }
    }
}
