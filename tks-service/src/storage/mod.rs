use lazy_static::lazy_static;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::DirBuilder;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec::Vec;

use crate::settings::SETTINGS;

#[derive(Serialize, Deserialize, Debug)]
pub struct ItemData {
    parameters: Vec<u8>,
    data: Vec<u8>,
    pub content_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Item {
    pub label: String,
    pub created: u64,
    pub modified: u64,
    #[serde(skip)]
    pub data: Option<ItemData>, // when Item is locked, this is None

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Collection {
    schema_version: u8,
    pub name: String,
    pub items: Option<Vec<Item>>,
    aliases: Option<Vec<String>>,
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
            path: OsString::from(SETTINGS.lock().unwrap().storage.path.clone()),
            collections: Vec::new(),
        };
        // check if the storage directory exists
        // if not, create it
        let _ = DirBuilder::new()
            .recursive(true)
            .create(storage.path.clone())?;

        let paths = std::fs::read_dir(storage.path.clone()).unwrap();
        for path in paths {
            let collection_path = path.unwrap().path();
            storage
                .collections
                .push(Self::load_collection(&collection_path).unwrap());
        }

        // look for the default collection and create it if it doesn't exist
        match storage.read_alias("default") {
            Ok(name) => name,
            Err(_) => {
                debug!("Creating default collection");
                let _ = storage.create_collection("default", "default", &HashMap::new());
                "default".to_string()
            }
        };

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

    pub fn with_collection<F, T>(&mut self, alias: &str, f: F) -> Result<T, std::io::Error>
    where
        F: FnOnce(&mut Collection) -> T,
    {
        let mut collection = self
            .collections
            .iter_mut()
            .find(|c| c.name == alias)
            .ok_or(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Collection '{}' not found", alias),
            ))?;
        Ok(f(&mut collection))
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
        F: FnOnce(&Item) -> T,
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
        Ok(f(&item))
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
        let mut coll = Collection::new(name, collection_path.as_os_str());
        match alias {
            "" => {}
            _ => {
                coll.aliases = Some(vec![alias.to_string()]);
            }
        }
        Self::save_collection(&mut coll)?;
        self.collections.push(coll);
        debug!(
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
        debug!(
            "Saving collection '{}' to path '{}'",
            collection.name,
            metadata_path.display()
        );
        let mut file = File::create(metadata_path)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .into();
        collection.modified = ts;
        serde_json::to_writer_pretty(&mut file, collection)?;
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
        Ok(collection)
    }
}

impl Collection {
    fn new(name: &str, path: &OsStr) -> Collection {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
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

        collection
    }

    pub fn create_item(
        &mut self,
        label: &str,
        properties: HashMap<String, String>,
        secret: (dbus::Path<'static>, Vec<u8>, Vec<u8>, String),
        replace: bool,
    ) -> Result<(), std::io::Error> {
        if self.locked {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Collection is locked"),
            ));
        }
        let secret_session = secret.0.to_string();

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .into();
        let item = Item {
            label: label.to_string(),
            created: ts,
            modified: ts,
            data: Some(ItemData {
                parameters: secret.1,
                data: secret.2,
                content_type: secret.3,
            }),
            attributes: properties,
        };
        match self.items.as_mut() {
            Some(items) => {
                let index = items.iter().position(|i| i.attributes == item.attributes);
                match index {
                    Some(index) => {
                        if replace {
                            items[index] = item;
                        } else {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::AlreadyExists,
                                format!("Item already exists"),
                            ));
                        }
                    }
                    None => {
                        items.push(item);
                    }
                }
            }
            None => {
                self.items = Some(vec![item]);
            }
        }
        Storage::save_collection(self)?;
        Ok(())
    }

    pub fn delete_item(&mut self, label: &str) -> Result<(), std::io::Error> {
        if self.locked {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Collection is locked"),
            ));
        }
        match self.items.as_mut() {
            Some(items) => {
                let index = items.iter().position(|i| i.label == label);
                match index {
                    Some(index) => {
                        items.remove(index);
                    }
                    None => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Item not found"),
                        ));
                    }
                }
            }
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Item not found"),
                ));
            }
        }
        Storage::save_collection(self)?;
        Ok(())
    }
}

impl Item {
    pub fn get_secret(
        &self,
        session: &str,
    ) -> Result<(String, Vec<u8>, Vec<u8>, String), std::io::Error> {
        debug!("get_secret called on '{}'", self.label);
        // TODO here we should check if the session is authorized to access this item and use to
        // decrypt the secret
        match &self.data {
            Some(data) => Ok((
                session.to_string(),
                data.parameters.clone(),
                data.data.clone(),
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
        session: String,
        parameters: Vec<u8>,
        value: Vec<u8>,
        content_type: String,
    ) -> Result<(), std::io::Error> {
        debug!("set_secret called on '{}'", self.label);
        // TODO here we should check if the session is authorized to access this item and use to
        // encrypt the secret
        match &mut self.data {
            Some(data) => {
                data.parameters = parameters;
                data.data = value;
                data.content_type = content_type;
                Ok(())
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Item is locked"),
            )),
        }
    }
}
