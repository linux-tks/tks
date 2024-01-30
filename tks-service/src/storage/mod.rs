use crate::tks_dbus::session_impl::Session;
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::fs::DirBuilder;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;
use uuid::Uuid;

use crate::settings::SETTINGS;
use crate::TksError;

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
    path: OsString,
    #[serde(skip)]
    pub locked: bool,
}

pub struct Storage {
    path: OsString,
    pub collections: Vec<Collection>,
}

lazy_static! {
    pub static ref STORAGE: Arc<Mutex<Storage>> = Arc::new(Mutex::new(Storage::new()));
}

impl Storage {
    fn new() -> Self {
        let do_create_storage = || {
            let mut storage = Storage {
                path: OsString::from(
                    SETTINGS
                        .lock()
                        .map_err(|e| {
                            std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Error getting settings: {}", e),
                            )
                        })?
                        .storage
                        .path
                        .clone(),
                ),
                collections: Vec::new(),
            };
            // check if the storage directory exists
            // if not, create it
            let _ = DirBuilder::new()
                .recursive(true)
                .create(storage.path.clone())?;

            let paths = std::fs::read_dir(storage.path.clone())?;
            for path in paths {
                let collection_path = path?.path();
                storage
                    .collections
                    .push(Self::load_collection(&collection_path)?);
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

    pub fn with_collection<F, T>(&mut self, uuid: Uuid, f: F) -> Result<T, TksError>
    where
        F: FnOnce(&mut Collection) -> Result<T, TksError>,
    {
        let mut collection = self
            .collections
            .iter_mut()
            .find(|c| c.uuid == uuid)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Collection '{}' not found", uuid),
                )
            })?;
        f(&mut collection)
    }

    pub fn modify_collection<F>(&mut self, uuid: &Uuid, f: F) -> Result<(), TksError>
    where
        F: FnOnce(&mut Collection) -> Result<(), TksError>,
    {
        self.collections
            .iter_mut()
            .find(|c| c.uuid == *uuid)
            .ok_or(TksError::NotFound(
                format!("Collection '{}' not found", uuid).into(),
            ))
            .and_then(|collection| {
                f(collection)?;
                Ok(collection)
            })
            .and_then(|collection| {
                // TODO the collection name may have changed; in this case, we might need to also
                // update the collection's path on disk; but for the moment, it should still reload
                // fine as the correct collection name gets serialized on disk
                Storage::save_collection(collection)?;
                Ok(())
            })
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
        let item = collection
            .items
            .iter_mut()
            .find(|i| i.id.uuid == *item_uuid)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item '{}' not found", item_uuid),
            ))?;
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
                    format!("Collection not found"),
                )
            })?;
        let mut item = collection
            .items
            .iter_mut()
            .find(|i| i.id.uuid == *item_uuid)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item '{}' not found", item_uuid),
            ))?;
        match f(&mut item) {
            Ok(t) => {
                item.modified = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .into();
                Storage::save_collection(collection)?;
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
        let mut collection_path = PathBuf::new();
        collection_path.push(self.path.clone());
        collection_path.push(name);
        let mut coll = Collection::new(name, collection_path.as_os_str())?;
        if !alias.is_empty() {
            coll.aliases = Some(vec![alias.to_string()]);
        }
        let uuid = coll.uuid;
        Self::save_collection(&mut coll)?;
        self.collections.push(coll);
        trace!(
            "Created collection '{}' at path '{}'",
            uuid,
            collection_path.display()
        );
        Ok(uuid)
    }

    fn save_collection(collection: &mut Collection) -> Result<(), TksError> {
        assert!(!collection.path.is_empty());
        let _ = DirBuilder::new()
            .recursive(true)
            .create(collection.path.clone())?;
        let mut metadata_path = PathBuf::new();
        metadata_path.push(collection.path.clone());
        metadata_path.push("metadata.json");
        trace!(
            "Saving collection '{}' to path '{}'",
            collection.name,
            metadata_path.display()
        );
        let mut file = File::create(metadata_path)?;
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
            let mut items_path = PathBuf::new();
            items_path.push(collection.path.clone());
            items_path.push("items.json");
            debug!("Collection items path: {}", items_path.display());
            let mut file = File::create(items_path)?;
            let collection_secrets = collection.get_secrets();
            serde_json::to_writer_pretty(&mut file, &collection_secrets)?;
        }
        Ok(())
    }

    fn load_collection(path: &PathBuf) -> Result<Collection, TksError> {
        trace!("Loading collection from path '{}'", path.display());
        let mut metadata_path = PathBuf::new();
        metadata_path.push(path);
        metadata_path.push("metadata.json");

        let mut file = File::open(metadata_path)?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let mut collection: Collection = serde_json::from_str(&data)?;
        collection.path = path.as_os_str().to_os_string();
        collection.locked = true;
        collection
            .items
            .iter_mut()
            .for_each(|i: &mut Item| i.id.collection_uuid = collection.uuid);
        Ok(collection)
    }
}

impl Collection {
    fn new(name: &str, path: &OsStr) -> Result<Collection, TksError> {
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
            path: path.to_os_string(),
            items: Vec::new(),
            aliases: None,
            locked: true,
            created: ts,
            modified: ts,
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
        Storage::save_collection(self)?;
        Ok(item_id)
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
                Storage::save_collection(self)?;
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
        let mut items_path = PathBuf::new();
        items_path.push(self.path.clone());
        items_path.push("items.json");
        let data = fs::read_to_string(items_path)
            .ok()
            .or(Some("".to_string()))
            .unwrap();
        let collection_secrets: CollectionSecrets = serde_json::from_str(&data)
            .ok()
            .or(Some(CollectionSecrets::new()))
            .unwrap();

        self.items.iter_mut().for_each(|item| {
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
        self.locked = false;
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
