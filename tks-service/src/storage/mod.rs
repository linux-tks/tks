use crate::tks_dbus::session_impl::Session;
use lazy_static::lazy_static;
use log::{debug, error, trace};
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ItemData {
    uuid: Uuid,
    parameters: Vec<u8>,
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
    pub data_uuid: Option<Uuid>,
    // when Item is locked, this is None
    #[serde(skip)]
    pub data: Option<ItemData>,

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Collection {
    schema_version: u8,
    pub name: String,
    pub items: Option<Vec<Item>>,
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
    pub static ref STORAGE: Arc<Mutex<Storage>> = Arc::new(Mutex::new(Storage::new().unwrap()));
}

impl Storage {
    fn new() -> Result<Self, std::io::Error> {
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
            error!("Creating default collection");
            storage
                .create_collection("default", "default", &HashMap::new())
                .map(|_| "default".to_string())
        })?;

        Ok(storage)
    }

    pub fn read_alias(&mut self, alias: &str) -> Result<String, std::io::Error> {
        self.collections
            .iter()
            .filter(|c| c.aliases.is_some())
            .find(|&c| c.aliases.as_ref().unwrap().contains(&alias.to_string()))
            .map(|c| c.name.clone())
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Alias '{}' not found", alias),
            ))
    }

    pub fn with_collection<F, T>(&mut self, alias: String, f: F) -> Result<T, std::io::Error>
    where
        F: FnOnce(&mut Collection) -> Result<T, std::io::Error>,
    {
        let mut collection = self
            .collections
            .iter_mut()
            .find(|c| c.name == alias)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Collection '{}' not found", alias),
                )
            })?;
        f(&mut collection)
    }

    pub fn modify_collection<F>(&mut self, alias: &str, f: F) -> Result<(), std::io::Error>
    where
        F: FnOnce(&mut Collection) -> Result<(), std::io::Error>,
    {
        self.collections
            .iter_mut()
            .find(|c| c.name == alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Collection '{}' not found", alias),
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
        collection_alias: &str,
        item_alias: &str,
        f: F,
    ) -> Result<T, std::io::Error>
    where
        F: FnOnce(&Item) -> Result<T, std::io::Error>,
    {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.name == collection_alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Collection '{}' not found", collection_alias),
            ))?;
        let item = collection
            .items
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|i| i.label == item_alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item '{}' not found", item_alias),
            ))?;
        f(&item)
    }

    pub fn modify_item<F, T>(
        &mut self,
        collection_alias: &str,
        item_alias: &str,
        f: F,
    ) -> Result<T, std::io::Error>
    where
        F: FnOnce(&mut Item) -> Result<T, std::io::Error>,
    {
        let collection = self
            .collections
            .iter_mut()
            .find(|c| c.name == collection_alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Collection '{}' not found", collection_alias),
            ))?;
        let mut item = collection
            .items
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|i| i.label == item_alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item '{}' not found", item_alias),
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
    ) -> Result<(), std::io::Error> {
        let mut collection_path = PathBuf::new();
        collection_path.push(self.path.clone());
        collection_path.push(name);
        let mut coll = Collection::new(name, collection_path.as_os_str())?;
        if !alias.is_empty() {
            coll.aliases = Some(vec![alias.to_string()]);
        }
        Self::save_collection(&mut coll)?;
        self.collections.push(coll);
        trace!(
            "Created collection '{}' at path '{}'",
            name,
            collection_path.display()
        );
        Ok(())
    }

    fn save_collection(collection: &mut Collection) -> Result<(), std::io::Error> {
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

    fn load_collection(path: &PathBuf) -> Result<Collection, std::io::Error> {
        let mut metadata_path = PathBuf::new();
        metadata_path.push(path);
        metadata_path.push("metadata.json");

        let mut file = File::open(metadata_path)?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let mut collection: Collection = serde_json::from_str(&data)?;
        collection.path = path.as_os_str().to_os_string();
        collection.locked = true;
        Ok(collection)
    }
}

impl Collection {
    fn new(name: &str, path: &OsStr) -> Result<Collection, std::io::Error> {
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
            schema_version: 1,
            name: name.to_string(),
            path: path.to_os_string(),
            items: None,
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
    ) -> Result<(), std::io::Error> {
        if self.locked {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Collection is locked"),
            ));
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
                parameters: secret.1.clone(),
                data: match secret_session.decrypt(&secret.1, &secret.2) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Cannot decrypt secret: {}", e);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("Error decrypting secret: {}", e),
                        ));
                    }
                },
                content_type: secret.3,
            }),
            data_uuid: Some(uuid),
            attributes: properties,
        };
        match self.items.as_mut() {
            Some(items) => {
                if let Some(index) = items.iter().position(|i| i.attributes == item.attributes) {
                    if replace {
                        items[index] = item;
                    } else {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            format!("Item already exists"),
                        ));
                    }
                } else {
                    items.push(item);
                }
            }
            None => {
                self.items = Some(vec![item]);
            }
        }
        Storage::save_collection(self)
    }

    pub fn delete_item(&mut self, label: &str) -> Result<(), std::io::Error> {
        if self.locked {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Collection is locked"),
            ));
        }
        self.items
            .as_ref()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("Item not found"))
            })
            .unwrap()
            .iter()
            .position(|i| i.label == label)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("Item not found"))
            })
            .and_then(|i| {
                self.items.as_mut().unwrap().swap_remove(i);
                Storage::save_collection(self)?;
                Ok(())
            })
    }

    fn get_secrets(&self) -> CollectionSecrets {
        let mut secrets = CollectionSecrets::new();
        match &self.items {
            Some(items) => {
                for item in items {
                    match &item.data {
                        Some(data) => secrets.items.push((*data).clone()),
                        None => {}
                    }
                }
            }
            None => {}
        }
        secrets
    }

    pub fn unlock(&mut self) -> Result<(), std::io::Error> {
        if !self.locked {
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
        match &mut self.items {
            Some(items) => {
                for item in items {
                    match &mut item.data_uuid {
                        Some(uuid) => {
                            let secret = collection_secrets
                                .items
                                .iter()
                                .find(|s| s.uuid == *uuid)
                                .ok_or(std::io::Error::new(
                                    // TODO: maybe we should put the service in a fail state if we can't unlock a collection
                                    std::io::ErrorKind::NotFound,
                                    format!("Secrets file does not contain secret for item "),
                                ))?;
                            item.data = Some(secret.clone());
                        }
                        None => {}
                    }
                }
            }
            None => {}
        }
        self.locked = false;
        Ok(())
    }
    pub fn lock(&mut self) -> Result<bool, std::io::Error> {
        self.locked = true;
        match &mut self.items {
            Some(items) => {
                for item in items {
                    item.data = None;
                }
                Ok(true)
            }
            None => Ok(true),
        }
    }
}

impl Item {
    pub fn get_secret(
        &self,
        session: &Session,
    ) -> Result<(String, Vec<u8>, Vec<u8>, String), std::io::Error> {
        trace!("get_secret called on '{}'", self.label);
        match &self.data {
            Some(data) => Ok((
                "".to_string(),
                data.parameters.clone(),
                match session.encrypt(&data.data) {
                    Ok(data) => data,
                    Err(e) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("Error encrypting secret: {}", e),
                        ));
                    }
                },
                data.content_type.clone(),
            )),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item is locked"),
            )),
        }
    }
    pub fn set_secret(
        &mut self,
        session: &Session,
        parameters: Vec<u8>,
        value: &Vec<u8>,
        content_type: String,
    ) -> Result<(), std::io::Error> {
        trace!("set_secret called on '{}'", self.label);
        self.data = Some(ItemData {
            uuid: self.data_uuid.unwrap_or_else(|| Uuid::new_v4()),
            parameters: parameters.clone(),
            data: match session.decrypt(&parameters, value) {
                Ok(data) => data,
                Err(e) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Error decrypting secret: {}", e),
                    ));
                }
            },
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
